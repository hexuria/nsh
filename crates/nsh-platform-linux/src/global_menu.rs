//! The application menu in KDE Plasma's Global Menu (the "Global Menu"
//! applet or widget of a panel, at the top of the screen).
//!
//! - **When**: `XDG_CURRENT_DESKTOP` names KDE and `NSH_GLOBAL_MENU` is not
//!   `0` (asked once per process), while `com.canonical.AppMenu.Registrar`
//!   (kded's appmenu module, there while a Global Menu is in a panel) has an
//!   owner on the session bus: asked when first needed, then watched, so a
//!   widget added or removed while the app runs moves the menu to the panel
//!   or back to the window ([`available`], [`crate::chrome::menu_place_changed`]).
//!   Elsewhere nothing here runs and the menu stays the window's
//!   ([`crate::chrome`]).
//! - **What the app sees**: `Menu.inGlobalMenu()`
//!   ([`nsh_core::platform::Environment::global_menu`]) is true while the
//!   menu is exported and the window tied to it.
//! - **What**: the menu set with `setApplicationMenu` is exported as a
//!   `com.canonical.dbusmenu` object (`/com/canonical/menu/nsh`) on the
//!   app's session bus connection, numbered and answered as the tray's menu
//!   is ([`crate::tray::number`], [`crate::tray::dbusmenu_call`]), with
//!   the items' shortcuts and without the roles Linux has no use for. Each
//!   change bumps the revision and emits `LayoutUpdated`; a chosen item is
//!   carried out as the menu bar's are ([`crate::chrome::choose`]).
//! - **The window**: on Wayland, KWin's `org_kde_kwin_appmenu` (protocol
//!   `appmenu`) ties the window's `wl_surface` to the bus name and path;
//!   on X11, the registrar's `RegisterWindow(xid, path)`. Done again when
//!   GTK makes the window's surface anew (it realizes it again).
//! - While exported, GTK's `PopoverMenuBar` never shows (the menu is in the
//!   panel); its shortcuts stay on the window.
//! - **One export a thread**: with several surfaces (an app's window and
//!   an embedded view), the export is the surface's that has a menu, the
//!   last to set one when several do ([`export_owner`]): whichever order
//!   surfaces are walked in ([`registrar_changed`]), any other leaves it
//!   be. Only the exporting surface hides its bar and says
//!   `Menu.inGlobalMenu()`.
//!
//! Without the `global-menu` feature (no X11 or Wayland bindings), the
//! window cannot be tied to the menu, so [`available`] is false.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::OnceLock,
};

use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use nsh_core::desktop::{MenuItem, MenuRole};

use crate::{
    surface::Surface,
    tray::{self, Entry},
};

const REGISTRAR: &str = "com.canonical.AppMenu.Registrar";
const REGISTRAR_PATH: &str = "/com/canonical/AppMenu/Registrar";
/// Apart from the tray's menu (`/MenuBar`).
const MENU_PATH: &str = "/com/canonical/menu/nsh";

/// The rule: KDE, no opt-out, then (only then asked) the registrar.
pub(crate) fn rule(
    current_desktop: Option<&str>,
    opt_out: Option<&str>,
    registrar: impl FnOnce() -> bool,
) -> bool {
    if opt_out.is_some_and(|v| v.trim() == "0") {
        return false;
    }
    let kde = current_desktop.is_some_and(|d| {
        d.split(':')
            .any(|part| part.trim().eq_ignore_ascii_case("kde"))
    });
    kde && registrar()
}

/// KDE, no opt-out, built with the feature: whether to look for a Global
/// Menu at all (asked once per process).
fn wanted() -> bool {
    static ANSWER: OnceLock<bool> = OnceLock::new();
    *ANSWER.get_or_init(|| {
        cfg!(feature = "global-menu")
            && rule(
                std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
                std::env::var("NSH_GLOBAL_MENU").ok().as_deref(),
                || true,
            )
    })
}

/// Whether the registrar is on the bus: asked the first time, then
/// followed ([`registrar_changed`]).
fn registrar_up() -> bool {
    if let Some(up) = REGISTRAR_UP.get() {
        return up;
    }
    // Asked at once (one round trip), so the first frame knows where the
    // menu is; then followed.
    let up = crate::system::name_has_owner(REGISTRAR);
    REGISTRAR_UP.set(Some(up));
    crate::system::watch_name(REGISTRAR, Some(up), registrar_changed);
    up
}

/// A Global Menu widget came or went: every surface places its menu again
/// (exported and tied, or back in the window's bar).
fn registrar_changed(up: bool) {
    REGISTRAR_UP.set(Some(up));
    for surface in crate::system::surfaces() {
        crate::chrome::menu_place_changed(&surface);
    }
}

/// The Global Menu takes the application menu in this process now.
pub(crate) fn available() -> bool {
    wanted() && registrar_up()
}

/// The menu while exported.
struct Export {
    surface: Weak<Surface>,
    bus: gio::DBusConnection,
    /// The dbusmenu object, unregistered when the export ends.
    registration: Option<gio::RegistrationId>,
    entries: Vec<Entry>,
    revision: u32,
    /// The window surface the menu is tied to, and how.
    tied: Option<(glib::WeakRef<gdk::Surface>, Tie)>,
}

/// How the window is tied to the menu (kept for as long as it is).
#[cfg_attr(not(feature = "global-menu"), allow(dead_code))]
enum Tie {
    /// Held for its `Drop`, which releases KWin's appmenu object.
    #[cfg(feature = "global-menu")]
    Wayland(#[allow(dead_code)] wayland::Appmenu),
    /// The registrar's `RegisterWindow`; `Drop` takes it back
    /// (`UnregisterWindow`), so a handover frees the previous window.
    X11 {
        #[allow(dead_code)]
        xid: u32,
        #[allow(dead_code)]
        bus: gio::DBusConnection,
        /// This `RegisterWindow` call's own number: a refusal of an
        /// earlier registration must not undo this newer tie.
        #[allow(dead_code)]
        registration: u64,
    },
    /// Neither backend could (an unknown GDK backend): nothing shows.
    None,
}

#[cfg(feature = "global-menu")]
impl Drop for Tie {
    fn drop(&mut self) {
        if let Tie::X11 { xid, bus, .. } = self {
            unregister_x11(bus, *xid);
        }
    }
}

impl Drop for Export {
    fn drop(&mut self) {
        if let Some(id) = self.registration.take() {
            let _ = self.bus.unregister_object(id);
        }
    }
}

thread_local! {
    static EXPORT: RefCell<Option<Export>> = const { RefCell::new(None) };
    /// The registrar is on the bus (`None`: not asked yet).
    static REGISTRAR_UP: Cell<Option<bool>> = const { Cell::new(None) };
    /// Each `RegisterWindow` call's own number: a refusal answered late
    /// belongs to one tie, not to whichever the window has now.
    static NEXT_REGISTRATION: Cell<u64> = const { Cell::new(0) };
}

/// `surface`'s menu is in the Global Menu: exported, and its window tied
/// to it. That window then shows no menu bar, and its `Menu.inGlobalMenu()`
/// is true. The export is one per thread: another surface's window keeps
/// its own bar.
pub(crate) fn active(surface: &Surface) -> bool {
    available() && owned(surface)
}

/// The export is `surface`'s, its window tied to the menu.
fn owned(surface: &Surface) -> bool {
    EXPORT.with(|e| {
        e.borrow().as_ref().is_some_and(|x| {
            std::ptr::eq(x.surface.as_ptr(), surface)
                && x.tied
                    .as_ref()
                    .is_some_and(|(_, tie)| !matches!(tie, Tie::None))
        })
    })
}

/// Tells the app where its menu is (`Menu.inGlobalMenu()`), the Global
/// Menu's presence answered as `up`.
fn report(surface: &Surface, up: bool) {
    let mut env = surface.environment.get();
    env.global_menu = up && owned(surface);
    surface.set_environment(env);
}

/// The surface the export is for, while it lives.
fn owner() -> Option<Rc<Surface>> {
    EXPORT.with(|e| e.borrow().as_ref().and_then(|x| x.surface.upgrade()))
}

/// The surface the export belongs to: of those with a menu, the last to
/// set one ([`crate::chrome::menu_order`]).
fn export_owner() -> Option<Rc<Surface>> {
    crate::system::surfaces()
        .into_iter()
        .filter(|s| crate::chrome::has_menu(s))
        .max_by_key(|s| crate::chrome::menu_order(s))
}

/// Exports `items` (none: an empty menu) for `window`'s Global Menu, and
/// ties the window to it. False when the Global Menu is not there.
pub(crate) fn export(
    surface: &Rc<Surface>,
    window: &gtk::Window,
    items: Option<&[MenuItem]>,
) -> bool {
    place(surface, window, items, available())
}

/// `export`, with the Global Menu's presence answered (`up` — the tests
/// answer it).
fn place(
    surface: &Rc<Surface>,
    window: &gtk::Window,
    items: Option<&[MenuItem]>,
    up: bool,
) -> bool {
    if !up {
        // The widget went (or never was): the export ends, the window's
        // menu bar takes the menu.
        drop(EXPORT.with(|e| e.borrow_mut().take()));
        report(surface, up);
        return false;
    }
    let export_to = export_owner();
    if let Some(wanted) = export_to.as_ref()
        && !Rc::ptr_eq(wanted, surface)
    {
        // Not this surface's: the export is the last setter's, whichever
        // order surfaces were walked in.
        if !owner().is_some_and(|o| Rc::ptr_eq(&o, wanted)) {
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            if let Some(window) = crate::chrome::window(wanted) {
                let items = crate::chrome::application_menu(wanted);
                if place(wanted, &window, items.as_deref(), up) {
                    crate::chrome::show_menu_bar(wanted);
                }
            }
        }
        report(surface, up);
        return false;
    }
    if export_to.is_none() && items.is_none() {
        // No menu anywhere (the last `setApplicationMenu(null)` cleared
        // it): the export ends rather than publish an empty menu, and the
        // window's menu bar takes over.
        let before = owner().filter(|o| !Rc::ptr_eq(o, surface));
        drop(EXPORT.with(|e| e.borrow_mut().take()));
        report(surface, up);
        if let Some(before) = before {
            report(&before, up);
            crate::chrome::show_menu_bar(&before);
        }
        return false;
    }
    // The surface that had the export, when it passes to this one.
    let before = owner().filter(|o| !Rc::ptr_eq(o, surface));
    let entries = tray::number(items.unwrap_or(&[]), true);
    let made = EXPORT.with(|e| {
        let mut e = e.borrow_mut();
        if let Some(export) = e.as_mut() {
            export.surface = Rc::downgrade(surface);
            export.entries = entries;
            export.revision += 1;
            let _ = export.bus.emit_signal(
                None,
                MENU_PATH,
                tray::MENU_INTERFACE,
                "LayoutUpdated",
                Some(&(export.revision, 0i32).to_variant()),
            );
            return true;
        }
        match publish(surface, entries) {
            Some(export) => {
                *e = Some(export);
                true
            }
            None => false,
        }
    });
    if made {
        tie(window);
    }
    report(surface, up);
    if let Some(before) = before {
        // Its menu left the panel: its window's bar shows again.
        report(&before, up);
        crate::chrome::show_menu_bar(&before);
    }
    made
}

/// Registers the dbusmenu object on the session bus.
fn publish(surface: &std::rc::Rc<Surface>, entries: Vec<Entry>) -> Option<Export> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|e| log::warn!("nsh: no session bus for the Global Menu: {e}"))
        .ok()?;
    let info = gio::DBusNodeInfo::for_xml(tray::XML).ok()?;
    let interface = info.lookup_interface(tray::MENU_INTERFACE)?;
    let registration = bus
        .register_object(MENU_PATH, &interface)
        .method_call(|_, _, _, _, method, params, invocation| {
            // Every item an `EventGroup` clicked, in order (outside the
            // borrow: `clicks` reads the export too).
            let mut ids = Vec::new();
            let reply = EXPORT.with(|e| {
                let e = e.borrow();
                let (entries, revision) = e
                    .as_ref()
                    .map_or((&[][..], 0), |x| (&x.entries[..], x.revision));
                tray::dbusmenu_call(entries, revision, method, &params, &mut |id| ids.push(id))
            });
            for (surface, action, role) in clicks(&ids) {
                dispatch(surface, action, role);
            }
            invocation.return_value(reply.as_ref());
        })
        .property(|_, _, _, _, property| tray::menu_property(property))
        .build()
        .map_err(|e| log::warn!("nsh: cannot export the menu to the Global Menu: {e}"))
        .ok()?;
    Some(Export {
        surface: std::rc::Rc::downgrade(surface),
        bus,
        registration: Some(registration),
        entries,
        revision: 1,
        tied: None,
    })
}

/// The action entry `id` carries out.
fn click_item(entries: &[Entry], id: i32) -> Option<(u32, Option<MenuRole>)> {
    entries
        .get(usize::try_from(id).ok()?.checked_sub(1)?)?
        .action
}

/// What each clicked `id` runs, all resolved in one read of the export —
/// the menu the click saw, before the first one's carrying out can
/// replace it.
fn clicks(ids: &[i32]) -> Vec<(Weak<Surface>, u32, Option<MenuRole>)> {
    EXPORT.with(|e| {
        let e = e.borrow();
        let Some(x) = e.as_ref() else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|&id| click_item(&x.entries, id))
            .map(|(action, role)| (x.surface.clone(), action, role))
            .collect()
    })
}

/// `action` was chosen in the panel, for `surface`.
fn dispatch(surface: Weak<Surface>, action: u32, role: Option<MenuRole>) {
    // Out of the bus call: the app's function may change the menu.
    glib::idle_add_local_once(move || {
        if let Some(surface) = surface.upgrade() {
            crate::chrome::choose(&surface, action, role);
        }
    });
}

/// Ties `window`'s surface to the menu, unless it already is.
fn tie(window: &gtk::Window) {
    let Some(gdk_surface) = window.surface() else {
        return;
    };
    let (bus, done) = EXPORT.with(|e| {
        let e = e.borrow();
        let x = e.as_ref();
        let done = x
            .and_then(|x| x.tied.as_ref())
            .is_some_and(|(s, _)| s.upgrade().as_ref() == Some(&gdk_surface));
        (x.map(|x| x.bus.clone()), done)
    });
    let Some(bus) = bus else {
        return;
    };
    if done {
        return;
    }
    let tie = tie_surface(&bus, window, &gdk_surface);
    EXPORT.with(|e| {
        if let Some(x) = e.borrow_mut().as_mut() {
            x.tied = Some((gdk_surface.downgrade(), tie));
        }
    });
}

#[cfg(feature = "global-menu")]
fn tie_surface(bus: &gio::DBusConnection, window: &gtk::Window, surface: &gdk::Surface) -> Tie {
    if let Some(service) = bus.unique_name()
        && let Some(menu) = wayland::tie(window, surface, service.as_str(), MENU_PATH)
    {
        return Tie::Wayland(menu);
    }
    if let Some(xid) = x11_window(surface) {
        let registration = NEXT_REGISTRATION.with(|n| n.replace(n.get() + 1));
        register_x11(bus, xid, registration);
        return Tie::X11 {
            xid,
            bus: bus.clone(),
            registration,
        };
    }
    log::warn!("nsh: the Global Menu cannot be tied to this window (neither Wayland nor X11)");
    Tie::None
}

#[cfg(not(feature = "global-menu"))]
fn tie_surface(_: &gio::DBusConnection, _: &gtk::Window, _: &gdk::Surface) -> Tie {
    Tie::None
}

/// The X11 window id of `surface`, on X11.
#[cfg(feature = "global-menu")]
fn x11_window(surface: &gdk::Surface) -> Option<u32> {
    let surface = surface.downcast_ref::<gdk4_x11::X11Surface>()?;
    u32::try_from(surface.xid()).ok()
}

/// Tells the registrar which menu window `xid` has.
#[cfg_attr(not(feature = "global-menu"), allow(dead_code))]
fn register_x11(bus: &gio::DBusConnection, xid: u32, registration: u64) {
    let Ok(path) = glib::variant::ObjectPath::try_from(MENU_PATH.to_owned()) else {
        return;
    };
    bus.call(
        Some(REGISTRAR),
        REGISTRAR_PATH,
        REGISTRAR,
        "RegisterWindow",
        Some(&(xid, path).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
        move |result| {
            if let Err(e) = result {
                log::warn!("nsh: the Global Menu's registrar refused the window: {e}");
                untie(registration);
            }
        },
    );
}

/// The registrar refused registration `number`: the window was never
/// tied after all — the export stays untied, the app is told the menu is
/// its window's again (`Menu.inGlobalMenu()` false) and the bar shows.
/// A newer tie of the same window has a newer number and is left alone.
#[cfg_attr(not(feature = "global-menu"), allow(dead_code))]
fn untie(number: u64) {
    let surface = EXPORT.with(|e| {
        let mut e = e.borrow_mut();
        let x = e.as_mut()?;
        let (_, tie) = x.tied.as_mut()?;
        if matches!(tie, Tie::X11 { registration, .. } if *registration == number) {
            *tie = Tie::None;
        }
        x.surface.upgrade()
    });
    if let Some(surface) = surface {
        report(&surface, true);
        crate::chrome::show_menu_bar(&surface);
    }
}

/// Tells the registrar window `xid` no longer has the menu.
#[cfg(feature = "global-menu")]
fn unregister_x11(bus: &gio::DBusConnection, xid: u32) {
    bus.call(
        Some(REGISTRAR),
        REGISTRAR_PATH,
        REGISTRAR,
        "UnregisterWindow",
        Some(&(xid,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
        |result| {
            if let Err(e) = result {
                // The registrar is gone half the times this runs (the
                // widget went, the export ended with it).
                log::debug!("nsh: the window was not untied from the Global Menu: {e}");
            }
        },
    );
}

/// KWin's `org_kde_kwin_appmenu`, on GTK's own Wayland connection.
#[cfg(feature = "global-menu")]
mod wayland {
    use gdk4_wayland::{WaylandDisplay, WaylandSurface, prelude::*};
    use gtk4::{self as gtk, gdk, prelude::*};
    use wayland_client::{
        Connection, Dispatch, EventQueue, Proxy, QueueHandle,
        globals::{GlobalListContents, registry_queue_init},
        protocol::wl_registry,
    };
    use wayland_protocols_plasma::appmenu::client::{
        org_kde_kwin_appmenu::OrgKdeKwinAppmenu,
        org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager,
    };

    /// The objects that keep the window tied (dropped: released).
    pub(super) struct Appmenu {
        _queue: EventQueue<State>,
        _manager: OrgKdeKwinAppmenuManager,
        menu: OrgKdeKwinAppmenu,
    }

    impl Drop for Appmenu {
        fn drop(&mut self) {
            self.menu.release();
        }
    }

    pub(super) struct State;

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    wayland_client::delegate_noop!(State: ignore OrgKdeKwinAppmenuManager);
    wayland_client::delegate_noop!(State: ignore OrgKdeKwinAppmenu);

    /// Ties the window's `wl_surface` to `service` and `path`; None off
    /// Wayland or without KWin's appmenu global.
    pub(super) fn tie(
        window: &gtk::Window,
        surface: &gdk::Surface,
        service: &str,
        path: &str,
    ) -> Option<Appmenu> {
        let display = WidgetExt::display(window)
            .downcast::<WaylandDisplay>()
            .ok()?;
        let surface = surface.downcast_ref::<WaylandSurface>()?;
        let wl_display = display.wl_display()?;
        let wl_surface = surface.wl_surface()?;
        let backend = wl_display.backend().upgrade()?;
        let connection = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<State>(&connection).ok()?;
        let handle = queue.handle();
        let manager: OrgKdeKwinAppmenuManager = globals.bind(&handle, 1..=2, ()).ok()?;
        let menu = manager.create(&wl_surface, &handle, ());
        menu.set_address(service.to_owned(), path.to_owned());
        queue.roundtrip(&mut State).ok()?;
        Some(Appmenu {
            _queue: queue,
            _manager: manager,
            menu,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use nsh_core::desktop::{DesktopCommand, MenuAction};

    use super::*;
    use crate::gtk_test;

    fn action(id: u32, label: &str) -> MenuItem {
        MenuItem::Action(MenuAction {
            id,
            label: label.into(),
            shortcut: None,
            enabled: true,
            checked: None,
            role: None,
            when: nsh_core::keymap::When::Always,
            icon: None,
            symbol: None,
            destructive: false,
        })
    }

    /// `SetMenu`, as the app's command does it.
    fn set_menu(surface: &Rc<Surface>, items: Option<Vec<MenuItem>>) {
        crate::chrome::command(
            surface,
            &DesktopCommand::SetMenu(items.map(|items| items.into())),
        );
    }

    /// `place` for each of the thread's surfaces, as `registrar_changed`
    /// does them — with `up` answered (there is no registrar on the
    /// tests' bus).
    fn place_every_surface(up: bool) {
        for surface in crate::system::surfaces() {
            let items = crate::chrome::application_menu(&surface);
            if let Some(window) = crate::chrome::window(&surface) {
                place(&surface, &window, items.as_deref(), up);
            }
        }
    }

    /// The session bus, or `None` where the test cannot publish (it does
    /// not run then, as `gtk_test::run` skips without a display).
    fn session_bus() -> Option<gio::DBusConnection> {
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
    }

    /// Whether the display backend is X11, the only backend the
    /// registrar's window registrations apply to.
    #[cfg(feature = "global-menu")]
    fn x11_display() -> bool {
        gdk::Display::default().is_some_and(|d| d.is::<gdk4_x11::X11Display>())
    }

    #[test]
    fn a_widget_keeps_the_panel_on_the_menu_set_last_not_the_last_surface() {
        gtk_test::run(|| {
            if session_bus().is_none() {
                return;
            }
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            // `surfaces()` lists `one` before `two`; `one` set its menu
            // most recently.
            let one = gtk_test::surface();
            let two = gtk_test::surface();
            crate::system::watch_environment(&one);
            crate::system::watch_environment(&two);
            let _w1 = gtk_test::show(&one);
            let _w2 = gtk_test::show(&two);
            set_menu(&two, Some(vec![action(1, "Second's")]));
            set_menu(&one, Some(vec![action(2, "First's")]));
            // A Global Menu widget appears.
            place_every_surface(true);
            assert!(
                owner().is_some_and(|o| Rc::ptr_eq(&o, &one)),
                "the last setter's menu is the panel's"
            );
            // (Without `global-menu` no window's menu is the panel's.)
            #[cfg(feature = "global-menu")]
            assert!(one.environment.get().global_menu);
            assert!(!two.environment.get().global_menu);
        });
    }

    #[test]
    fn the_export_follows_the_last_setter_when_the_owner_s_menu_goes() {
        gtk_test::run(|| {
            if session_bus().is_none() {
                return;
            }
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            let one = gtk_test::surface();
            let two = gtk_test::surface();
            crate::system::watch_environment(&one);
            crate::system::watch_environment(&two);
            let w1 = gtk_test::show(&one);
            let w2 = gtk_test::show(&two);
            set_menu(&two, Some(vec![action(1, "Second's")]));
            set_menu(&one, Some(vec![action(2, "First's")]));
            // `one`, the last setter, owns the export.
            place_every_surface(true);
            assert!(owner().is_some_and(|o| Rc::ptr_eq(&o, &one)));
            // `one`'s menu goes: the export moves to `two`, the only
            // surface left with a menu.
            set_menu(&one, None);
            place(&one, &w1, None, true);
            assert!(
                owner().is_some_and(|o| Rc::ptr_eq(&o, &two)),
                "the export is the last setter still standing"
            );
            assert!(!one.environment.get().global_menu);
            #[cfg(feature = "global-menu")]
            assert!(two.environment.get().global_menu);
            drop(w1);
            drop(w2);
        });
    }

    #[test]
    fn a_group_s_clicks_are_the_menu_the_click_saw() {
        gtk_test::run(|| {
            let Some(bus) = session_bus() else {
                return;
            };
            let surface = gtk_test::surface();
            // Entries 3 and 7 are actions; the click names them.
            let items: Vec<MenuItem> = (1..=7)
                .map(|i| action(i * 10, &format!("Item {i}")))
                .collect();
            let entries = tray::number(&items, true);
            EXPORT.with(|e| {
                *e.borrow_mut() = Some(Export {
                    surface: Rc::downgrade(&surface),
                    bus,
                    registration: None,
                    entries,
                    revision: 1,
                    tied: None,
                })
            });
            // An `EventGroup` with clicks of entries 3 and 7: every id
            // resolves to its action before any runs.
            let mut ids = Vec::new();
            let event = |id: i32| (id, "clicked".to_owned(), 0i32.to_variant(), 0u32);
            let params = (vec![event(3), event(7)],).to_variant();
            EXPORT.with(|e| {
                let e = e.borrow();
                let x = e.as_ref().unwrap();
                tray::dbusmenu_call(&x.entries, x.revision, "EventGroup", &params, &mut |id| {
                    ids.push(id)
                });
            });
            let chosen = clicks(&ids);
            // The first one's carrying out replaces the menu…
            EXPORT.with(|e| {
                e.borrow_mut().as_mut().unwrap().entries =
                    tray::number(&[action(99, "Another")], true);
            });
            // …and both clicks still run what the clicked menu had.
            assert_eq!(chosen.len(), 2);
            assert_eq!(chosen[0].1, 30);
            assert_eq!(chosen[1].1, 70);
            drop(EXPORT.with(|e| e.borrow_mut().take()));
        });
    }

    #[cfg(feature = "global-menu")]
    #[test]
    fn a_handover_unregisters_the_previous_window() {
        gtk_test::run(|| {
            let Some(bus) = session_bus() else {
                return;
            };
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            // X11-only: other backends do not register with the registrar.
            if !x11_display() {
                return;
            }
            // A registrar of the test's own, recording its calls.
            let (registration, calls) = fake_registrar(&bus, 0);

            let one = gtk_test::surface();
            let two = gtk_test::surface();
            crate::system::watch_environment(&one);
            crate::system::watch_environment(&two);
            // `one` exports first: `two` has no window yet, so `SetMenu`
            // only stores its menu.
            set_menu(&two, Some(vec![action(1, "Second's")]));
            let w1 = gtk_test::show(&one);
            set_menu(&one, Some(vec![action(2, "First's")]));
            let x1 = w1
                .surface()
                .and_then(|s| x11_window(&s))
                .expect("an X11 window");
            place(
                &one,
                &w1,
                crate::chrome::application_menu(&one).as_deref(),
                true,
            );
            assert!(owner().is_some_and(|o| Rc::ptr_eq(&o, &one)));
            assert!(
                gtk_test::wait_until(|| calls.borrow().iter().any(|(m, _)| m == "RegisterWindow")),
                "the first window registers"
            );
            // `two`'s `SetMenu` makes it the last setter; the export
            // moves to it and its window takes the registration.
            set_menu(&two, Some(vec![action(1, "Second's")]));
            let w2 = gtk_test::show(&two);
            let x2 = w2
                .surface()
                .and_then(|s| x11_window(&s))
                .expect("an X11 window");
            place(
                &two,
                &w2,
                crate::chrome::application_menu(&two).as_deref(),
                true,
            );
            assert!(owner().is_some_and(|o| Rc::ptr_eq(&o, &two)));
            assert!(!one.environment.get().global_menu);
            assert!(two.environment.get().global_menu);
            let until = || {
                let calls = calls.borrow();
                calls
                    .iter()
                    .any(|&(ref m, x)| m == "RegisterWindow" && x == x2)
                    && calls
                        .iter()
                        .any(|&(ref m, x)| m == "UnregisterWindow" && x == x1)
            };
            assert!(
                gtk_test::wait_until(until),
                "the previous window is untied: {:?}",
                calls.borrow()
            );
            let _ = bus.unregister_object(registration);
            release_name(&bus, REGISTRAR);
        });
    }

    /// The registrar's recorded `(method, window)` calls.
    #[cfg(feature = "global-menu")]
    type Calls = Rc<RefCell<Vec<(String, u32)>>>;

    /// The registrar's name and object, recorded method calls: a fake
    /// registrar on the tests' session bus. The first `failures`
    /// `RegisterWindow` calls are refused, the rest succeed.
    #[cfg(feature = "global-menu")]
    fn fake_registrar(bus: &gio::DBusConnection, failures: usize) -> (gio::RegistrationId, Calls) {
        let reply = bus
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "RequestName",
                Some(&(REGISTRAR, 0u32).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5000,
                gio::Cancellable::NONE,
            )
            .expect("the name answers");
        assert_eq!(
            reply.get::<(u32,)>().map(|(code,)| code),
            Some(1),
            "the registrar's name is owned"
        );
        let xml = "<node><interface name='com.canonical.AppMenu.Registrar'>\
                   <method name='RegisterWindow'><arg type='u' direction='in'/>\
                   <arg type='o' direction='in'/></method>\
                   <method name='UnregisterWindow'><arg type='u' direction='in'/></method>\
                   </interface></node>";
        let info = gio::DBusNodeInfo::for_xml(xml).unwrap();
        let interface = info.lookup_interface(REGISTRAR).unwrap();
        let calls: Rc<RefCell<Vec<(String, u32)>>> = Rc::default();
        let recorded = calls.clone();
        let registration = bus
            .register_object(REGISTRAR_PATH, &interface)
            .method_call(move |_, _, _, _, method, params, invocation| {
                recorded.borrow_mut().push((
                    method.to_owned(),
                    params.child_value(0).get::<u32>().unwrap_or(0),
                ));
                if recorded.borrow().len() > failures {
                    invocation.return_value(None);
                } else {
                    invocation.return_dbus_error("org.nsh.test.Refused", "the panel refused");
                }
            })
            .build()
            .expect("the registrar object registers");
        (registration, calls)
    }

    #[test]
    fn clearing_the_last_menu_ends_the_export() {
        gtk_test::run(|| {
            if session_bus().is_none() {
                return;
            }
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            let surface = gtk_test::surface();
            crate::system::watch_environment(&surface);
            let window = gtk_test::show(&surface);
            set_menu(&surface, Some(vec![action(1, "One's")]));
            place(&surface, &window, Some(&[action(1, "One's")]), true);
            assert!(owner().is_some());
            // Only with the feature does the panel claim the window's
            // menu (without `global-menu` the tie is `Tie::None`).
            #[cfg(feature = "global-menu")]
            assert!(surface.environment.get().global_menu);
            // The app clears the only menu: the export ends rather than
            // stay an empty menu, and `Menu.inGlobalMenu()` reads false.
            set_menu(&surface, None);
            place(&surface, &window, None, true);
            assert!(owner().is_none(), "no empty menu stays in the panel");
            assert!(!surface.environment.get().global_menu);
            drop(window);
        });
    }

    #[cfg(feature = "global-menu")]
    #[test]
    fn a_refused_registration_frees_the_window() {
        gtk_test::run(|| {
            let Some(bus) = session_bus() else {
                return;
            };
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            let surface = gtk_test::surface();
            crate::system::watch_environment(&surface);
            let window = gtk_test::show(&surface);
            // X11-only: other backends do not register with the registrar.
            if !x11_display() {
                return;
            }
            window
                .surface()
                .and_then(|s| x11_window(&s))
                .expect("an X11 window");
            let (registration, _calls) = fake_registrar(&bus, usize::MAX);
            set_menu(&surface, Some(vec![action(1, "One's")]));
            place(
                &surface,
                &window,
                crate::chrome::application_menu(&surface).as_deref(),
                true,
            );
            // Tied optimistically, before the registrar answers.
            assert!(surface.environment.get().global_menu);
            // The refusal frees the window: the menu is the window's
            // again (`Menu.inGlobalMenu()` false, the bar back).
            assert!(
                gtk_test::wait_until(|| !surface.environment.get().global_menu),
                "a refused window is not reported as exported"
            );
            let _ = bus.unregister_object(registration);
            release_name(&bus, REGISTRAR);
            drop(window);
        });
    }

    /// A refusal answered for an earlier `RegisterWindow` must not
    /// untie the registration the window has now.
    #[cfg(feature = "global-menu")]
    #[test]
    fn a_late_refusal_leaves_the_newer_tie() {
        gtk_test::run(|| {
            let Some(bus) = session_bus() else {
                return;
            };
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            let surface = gtk_test::surface();
            crate::system::watch_environment(&surface);
            let window = gtk_test::show(&surface);
            // X11-only: other backends do not register with the registrar.
            if !x11_display() {
                return;
            }
            let xid = window
                .surface()
                .and_then(|s| x11_window(&s))
                .expect("an X11 window");
            // The first registration is refused, the next succeeds.
            let (registration, calls) = fake_registrar(&bus, 1);
            set_menu(&surface, Some(vec![action(1, "One's")]));
            let items = crate::chrome::application_menu(&surface);
            place(&surface, &window, items.as_deref(), true);
            // The export ends and starts again for the same window
            // (registrar cycling, for instance): a new registration of
            // the same xid is pending before the first one's refusal
            // can land.
            drop(EXPORT.with(|e| e.borrow_mut().take()));
            place(&surface, &window, items.as_deref(), true);
            assert!(
                gtk_test::wait_until(|| calls
                    .borrow()
                    .iter()
                    .filter(|(m, _)| m == "RegisterWindow")
                    .count()
                    == 2),
                "the window registered twice: {:?}",
                calls.borrow()
            );
            // One more call, its reply ordered after the refused
            // call's: when it lands, the refusal's callback has run.
            let answered = Rc::new(Cell::new(false));
            let flag = answered.clone();
            bus.call(
                Some(REGISTRAR),
                REGISTRAR_PATH,
                REGISTRAR,
                "UnregisterWindow",
                Some(&(0u32,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5000,
                gio::Cancellable::NONE,
                move |_| flag.set(true),
            );
            assert!(
                gtk_test::wait_until(|| answered.get()),
                "the refused call's reply was handled"
            );
            assert!(
                surface.environment.get().global_menu,
                "the stale refusal left the newer tie alone"
            );
            assert_eq!(
                calls
                    .borrow()
                    .iter()
                    .filter(|&(m, x)| m == "UnregisterWindow" && *x == xid)
                    .count(),
                1,
                "only the dropped export unregistered: {:?}",
                calls.borrow()
            );
            let _ = bus.unregister_object(registration);
            release_name(&bus, REGISTRAR);
            drop(window);
        });
    }

    /// The name goes back to the bus (the connection is shared: another
    /// test owning it later would find it taken).
    #[cfg(feature = "global-menu")]
    fn release_name(bus: &gio::DBusConnection, name: &str) {
        let _ = bus.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ReleaseName",
            Some(&(name,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            gio::Cancellable::NONE,
        );
    }

    #[test]
    fn the_global_menu_needs_kde_and_the_registrar_and_no_opt_out() {
        assert!(rule(Some("KDE"), None, || true));
        assert!(rule(Some("ubuntu:KDE"), Some("1"), || true));
        assert!(!rule(Some("KDE"), None, || false));
        assert!(!rule(Some("GNOME"), None, || panic!("not asked off KDE")));
        assert!(!rule(None, None, || panic!("not asked")));
        assert!(!rule(Some("KDE"), Some("0"), || panic!(
            "not asked when opted out"
        )));
        assert!(!rule(Some("XFCE:KDEish"), None, || true));
    }
}
