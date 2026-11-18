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
//!
//! Without the `global-menu` feature (no X11 or Wayland bindings), the
//! window cannot be tied to the menu, so [`available`] is false.

use std::{
    cell::{Cell, RefCell},
    rc::Weak,
    sync::OnceLock,
};

use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use nsh_core::desktop::MenuItem;

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
    let up = crate::system::name_has_owner(REGISTRAR);
    REGISTRAR_UP.set(Some(up));
    // For the life of the process: its id is never given back.
    gio::bus_watch_name(
        gio::BusType::Session,
        REGISTRAR,
        gio::BusNameWatcherFlags::NONE,
        |_, _, _| registrar_changed(true),
        |_, _| registrar_changed(false),
    );
    up
}

/// A Global Menu widget came or went: every surface places its menu again
/// (exported and tied, or back in the window's bar).
fn registrar_changed(up: bool) {
    if REGISTRAR_UP.replace(Some(up)) == Some(up) {
        return;
    }
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
    X11,
    /// Neither backend could (an unknown GDK backend): nothing shows.
    None,
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
}

/// The menu is in the Global Menu: exported, and the window tied to it.
/// The window then shows no menu bar, and `Menu.inGlobalMenu()` is true.
pub(crate) fn active() -> bool {
    available()
        && EXPORT.with(|e| {
            e.borrow()
                .as_ref()
                .and_then(|x| x.tied.as_ref())
                .is_some_and(|(_, tie)| !matches!(tie, Tie::None))
        })
}

/// Tells the app where its menu is (`Menu.inGlobalMenu()`).
fn report(surface: &Surface) {
    let mut env = surface.environment.get();
    env.global_menu = active();
    surface.set_environment(env);
}

/// Exports `items` (none: an empty menu) for `window`'s Global Menu, and
/// ties the window to it. False when the Global Menu is not there.
pub(crate) fn export(
    surface: &std::rc::Rc<Surface>,
    window: &gtk::Window,
    items: Option<&[MenuItem]>,
) -> bool {
    if !available() {
        // The widget went (or never was): the export ends, the window's
        // menu bar takes the menu.
        drop(EXPORT.with(|e| e.borrow_mut().take()));
        report(surface);
        return false;
    }
    let entries = tray::number(items.unwrap_or(&[]), true);
    let made = EXPORT.with(|e| {
        let mut e = e.borrow_mut();
        if let Some(export) = e.as_mut() {
            export.surface = std::rc::Rc::downgrade(surface);
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
    report(surface);
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
            let mut chosen = None;
            let reply = EXPORT.with(|e| {
                let e = e.borrow();
                let (entries, revision) = e
                    .as_ref()
                    .map_or((&[][..], 0), |x| (&x.entries[..], x.revision));
                tray::dbusmenu_call(entries, revision, method, &params, &mut |id| {
                    chosen = Some(id)
                })
            });
            if let Some(id) = chosen {
                clicked(id);
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

/// Entry `id` was chosen in the panel.
fn clicked(id: i32) {
    let found = EXPORT.with(|e| {
        let e = e.borrow();
        let x = e.as_ref()?;
        let entry = x.entries.get(usize::try_from(id).ok()?.checked_sub(1)?)?;
        Some((x.surface.clone(), entry.action?))
    });
    let Some((surface, (action, role))) = found else {
        return;
    };
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
        register_x11(bus, xid);
        return Tie::X11;
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
fn register_x11(bus: &gio::DBusConnection, xid: u32) {
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
        |result| {
            if let Err(e) = result {
                log::warn!("nsh: the Global Menu's registrar refused the window: {e}");
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
    use super::*;

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
