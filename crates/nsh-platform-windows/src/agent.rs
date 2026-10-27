//! The platform's part of `nsh test --target windows`
//! (`nsh_testing::agent`): where a node's view is in the window, and the
//! input a user's hands give there, through the handlers the window's
//! messages go through ([`crate::synth`]). The agent calls from a thread
//! of its own; each call does its work on the main thread
//! ([`crate::window::invoke`]) and returns once it is done.

use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use nsh_core::{
    NodeId,
    geometry::{Bounds, Point, Points, Size, point},
};
use nsh_testing::agent::{Key, Native};

use crate::{
    input::{Modifiers, PointerKind, WheelEvent},
    surface::Surface,
    synth,
};

/// How long a call waits for the main thread at most.
const MAIN: Duration = Duration::from_secs(10);
/// A press's hold, as a quick click's.
const CLICK: Duration = Duration::from_millis(60);
/// A display frame, when the compositor cannot say when the next one is.
const FRAME: Duration = Duration::from_millis(16);
/// How far one wheel event moves at most, in points.
const SCROLL_STEP: f32 = 20.0;
/// Waits for the compositor's next frame, as input devices report once
/// per refresh: a fixed sleep (Win32 timers tick every ~15.6 ms) sent
/// fewer events than the display shows frames, so scrolled pages drew at
/// a fraction of its rate.
fn next_frame() {
    // SAFETY: a plain call; it fails while composition is off (a locked or
    // remote session).
    if unsafe { windows::Win32::Graphics::Dwm::DwmFlush() }.is_err() {
        std::thread::sleep(FRAME);
    }
}

/// The virtual keys the agent presses.
const VK_BACK: u16 = 0x08;
const VK_RETURN: u16 = 0x0D;
const VK_A: u16 = 0x41;

/// The surface the agent acts on, by key (its calls must be `Send`).
pub(crate) struct WindowsNative {
    key: u64,
    #[cfg(feature = "dev-agent")]
    available: Option<Arc<AtomicBool>>,
}

impl WindowsNative {
    pub(crate) fn new(key: u64) -> Self {
        WindowsNative {
            key,
            #[cfg(feature = "dev-agent")]
            available: None,
        }
    }

    #[cfg(feature = "dev-agent")]
    pub(crate) fn attached(key: u64, available: Arc<AtomicBool>) -> Self {
        WindowsNative {
            key,
            available: Some(available),
        }
    }

    #[cfg(feature = "dev-agent")]
    fn unavailable(&self) {
        if let Some(available) = &self.available {
            available.store(false, Ordering::Release);
        }
    }

    /// `f` with the surface on the main thread, waited for; `None` once
    /// the surface is gone.
    fn on_main<R: Send + 'static>(
        &self,
        f: impl FnOnce(&Rc<Surface>) -> R + Send + 'static,
    ) -> Option<R> {
        let (tx, rx) = mpsc::channel();
        let key = self.key;
        #[cfg(feature = "dev-agent")]
        let deadline = Instant::now() + MAIN;
        #[cfg(feature = "dev-agent")]
        let callback_available = self.available.clone();
        crate::window::invoke(move || {
            #[cfg(feature = "dev-agent")]
            if let Some(available) = &callback_available {
                if !dispatch_is_live(available, deadline) {
                    let _ = tx.send(None);
                    return;
                }
                let Some(surface) = Surface::by_key(key) else {
                    available.store(false, Ordering::Release);
                    let _ = tx.send(None);
                    return;
                };
                let _ = tx.send(Some(f(&surface)));
                return;
            }
            let _ = tx.send(Surface::by_key(key).map(|surface| f(&surface)));
        });
        let result = rx.recv_timeout(MAIN).ok().flatten();
        #[cfg(feature = "dev-agent")]
        if result.is_none() && self.available.is_some() {
            // A dispatch timeout makes the attached connection unavailable
            // until its host is restarted.
            self.unavailable();
        }
        result
    }

    fn click(&self, at: Point<Points>, hold: Duration, button: u8) {
        let (x, y) = (at.x.0, at.y.0);
        // A real pointer arrives before it presses (hover state).
        self.on_main(move |s| {
            synth::pointer(s, PointerKind::Move, x, y, 0, 0);
            synth::pointer(s, PointerKind::Down, x, y, button, 1 << button);
        });
        std::thread::sleep(hold);
        self.on_main(move |s| synth::pointer(s, PointerKind::Up, x, y, button, 0));
    }
}

impl Native for WindowsNative {
    #[cfg(feature = "dev-agent")]
    fn available(&self) -> bool {
        self.available
            .as_ref()
            .is_none_or(|available| available.load(Ordering::Acquire))
    }

    fn bounds(&self, id: NodeId) -> Option<Bounds<Points>> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(id)?;
            let to_root = view.to_root()?;
            let size = view.state().frame.size;
            mapped_bounds(
                |x, y| to_root.map(x, y),
                (0.0, 0.0, size.width.0, size.height.0),
            )
        })
        .flatten()
    }

    fn hit_at(&self, at: Point<Points>) -> Result<Option<NodeId>, String> {
        let (x, y) = (at.x.0, at.y.0);
        self.on_main(move |surface| crate::input::pick_node(&surface.root, x, y))
            .ok_or_else(|| "the window is gone".to_owned())
    }

    fn text_bounds(&self, text: NodeId, start: u32, end: u32) -> Option<Bounds<Points>> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(text)?;
            let rect = crate::text::range_rect(&view.state(), start, end)?;
            let to_root = view.to_root()?;
            mapped_bounds(|x, y| to_root.map(x, y), rect)
        })
        .flatten()
    }

    fn viewport(&self) -> Size<Points> {
        let (width, height) = self.on_main(|s| s.size()).unwrap_or((0.0, 0.0));
        Size::new(Points(width), Points(height))
    }

    fn press(&self, at: Point<Points>) {
        self.click(at, CLICK, 0);
    }

    fn long_press(&self, at: Point<Points>, hold: Duration) {
        self.click(at, hold, 0);
    }

    fn scroll(&self, at: Point<Points>, delta: Point<Points>) {
        let (x, y) = (at.x.0, at.y.0);
        let (dx, dy) = (delta.x.0, delta.y.0);
        // A precision touchpad's events, one per display refresh, adding up
        // to it.
        let n = (dx.abs().max(dy.abs()) / SCROLL_STEP).ceil().max(1.0) as u32;
        for _ in 0..n {
            self.on_main(move |s| {
                crate::input::wheel(
                    s,
                    WheelEvent {
                        x,
                        y,
                        dx: dx / n as f32,
                        dy: dy / n as f32,
                        precise: true,
                        modifiers: Modifiers::default(),
                        time: s.clock(),
                    },
                );
            });
            next_frame();
        }
    }

    fn drag(&self, from: Point<Points>, to: Point<Points>) {
        let (x, y) = (from.x.0, from.y.0);
        self.on_main(move |s| {
            synth::pointer(s, PointerKind::Move, x, y, 0, 0);
            synth::pointer(s, PointerKind::Down, x, y, 0, 1);
        });
        std::thread::sleep(nsh_testing::agent::LANDING);
        let mut last = from;
        for at in nsh_testing::agent::drag_path(from, to) {
            next_frame();
            let (x, y) = (at.x.0, at.y.0);
            // The primary button is held: a drag.
            self.on_main(move |s| synth::pointer(s, PointerKind::Move, x, y, 0, 1));
            last = at;
        }
        let (x, y) = (last.x.0, last.y.0);
        self.on_main(move |s| synth::pointer(s, PointerKind::Up, x, y, 0, 0));
    }

    fn type_text(&self, text: &str) {
        // Key by key, as a keyboard's characters come.
        for ch in text.chars() {
            self.on_main(move |s| synth::type_text(s, &ch.to_string()));
        }
    }

    fn key(&self, key: Key) {
        let (vk, modifiers) = match key {
            Key::Return => (VK_RETURN, Modifiers::default()),
            Key::Backspace => (VK_BACK, Modifiers::default()),
            Key::SelectAll => (
                VK_A,
                Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
            ),
        };
        self.on_main(move |s| synth::press(s, vk, modifiers));
    }

    fn mouse(&self) -> bool {
        true
    }

    fn right_click(&self, at: Point<Points>) -> Result<(), String> {
        // The secondary button.
        self.click(at, CLICK, 1);
        Ok(())
    }

    fn choose_menu_item(&self, path: &[String]) -> Result<(), String> {
        let path = path.to_vec();
        self.on_main(move |s| crate::chrome::choose_open_menu(s, &path))
            .unwrap_or_else(|| Err("the window is gone".to_owned()))
    }

    fn close_menu(&self) -> Result<(), String> {
        self.on_main(|s| crate::chrome::close_open_menu(s))
            .unwrap_or_else(|| Err("the window is gone".to_owned()))
    }

    fn hover(&self, at: Point<Points>) -> Result<(), String> {
        let (x, y) = (at.x.0, at.y.0);
        self.on_main(move |s| synth::pointer(s, PointerKind::Move, x, y, 0, 0))
            .ok_or_else(|| "the window is gone".to_owned())
    }

    fn press_keys(&self, keys: &str) -> Result<(), String> {
        let (vk, modifiers) = windows_key(keys)?;
        self.on_main(move |s| synth::press(s, vk, modifiers))
            .ok_or_else(|| "the window is gone".to_owned())
    }

    fn validate_keys(&self, keys: &str) -> Result<(), String> {
        windows_key(keys).map(|_| ())
    }

    fn focus_node(&self, id: NodeId) -> Result<(), String> {
        self.on_main(move |surface| {
            let view = surface
                .mounter
                .borrow()
                .views
                .get(id)
                .ok_or_else(|| format!("node {id} is missing"))?;
            crate::text_input::focus(&view);
            // Keys and text go to a web view's page while it holds the
            // Win32 focus, so the field needs the window's focus back.
            if synth::focused_child(surface).is_some() {
                // SAFETY: focusing the surface's own window on its thread.
                let _ = unsafe {
                    windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(surface.hwnd()))
                };
            }
            (crate::text_input::focused(surface).as_ref() == Some(&view)
                && synth::focused_child(surface).is_none())
            .then_some(())
            .ok_or_else(|| format!("node {id} cannot receive native focus"))
        })
        .unwrap_or_else(|| Err("the window is gone".to_owned()))
    }

    fn fire_menu_keybinding(
        &self,
        binding: &nsh_testing::agent::SemanticKeybinding,
    ) -> Result<(), String> {
        let binding = binding.clone();
        self.on_main(move |surface| crate::chrome::fire_menu_keybinding(surface, &binding))
            .unwrap_or_else(|| Err("the window is gone".into()))
    }

    fn app_focused(&self) -> bool {
        self.on_main(|surface| {
            // SAFETY: `GetForegroundWindow` is a read-only query.
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == surface.hwnd()
            }
        })
        .unwrap_or(false)
    }

    fn input_text(&self, id: NodeId) -> Option<String> {
        self.on_main(move |surface| {
            let view = surface.mounter.borrow().views.get(id)?;
            crate::text_input::current_text(&view)
        })
        .flatten()
    }

    fn back(&self) -> Result<(), String> {
        // The mouse's back button goes back from wherever the focus is
        // ([`crate::input`]), as in browsers.
        let middle = self.viewport();
        let at = point(Points(middle.width.0 / 2.0), Points(middle.height.0 / 2.0));
        self.click(at, CLICK, 3);
        Ok(())
    }

    fn answer_dialog(&self, button: Option<&str>) -> Result<(), String> {
        let button = button.map(str::to_owned);
        let (x, y) = self
            .on_main(move |s| crate::system::dialog_point(s, button.as_deref()))
            .unwrap_or_else(|| Err("the window is gone".into()))?;
        self.click(point(Points(x), Points(y)), CLICK, 0);
        Ok(())
    }

    fn animating(&self) -> bool {
        self.on_main(|s| s.animating()).unwrap_or(false)
    }

    fn screenshot(&self) -> Result<Vec<u8>, String> {
        self.on_main(synth::screenshot)
            .unwrap_or_else(|| Err("the window is gone".into()))
    }

    fn resize(&self, width: Points, height: Points) -> Result<(), String> {
        self.on_main(move |s| synth::resize(s, width.0, height.0))
            .ok_or_else(|| "the window is gone".to_owned())?
    }

    fn set_dark(&self, dark: bool) -> Result<(), String> {
        self.on_main(move |s| {
            crate::system::simulate_system_scheme(dark);
            crate::system::settings_changed(s);
        })
        .ok_or_else(|| "the window is gone".to_owned())
    }

    fn quit(&self) {
        self.on_main(|s| {
            // SAFETY: a plain call on the surface's window.
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(s.hwnd()) };
        });
    }
}

fn windows_key(keys: &str) -> Result<(u16, Modifiers), String> {
    let stroke = nsh_core::keymap::Keystroke::parse(keys, nsh_core::platform::Os::Windows)
        .map_err(|e| e.to_string())?;
    let vk = stroke
        .key
        .virtual_key()
        .ok_or_else(|| format!("{keys:?} has no key on a Windows keyboard"))?;
    let m = stroke.modifiers;
    Ok((
        vk,
        Modifiers {
            shift: m.shift,
            ctrl: m.ctrl,
            alt: m.alt,
            meta: m.cmd,
        },
    ))
}

#[cfg(feature = "dev-agent")]
fn dispatch_is_live(available: &AtomicBool, deadline: Instant) -> bool {
    if !available.load(Ordering::Acquire) || Instant::now() >= deadline {
        available.store(false, Ordering::Release);
        false
    } else {
        true
    }
}

/// The box `(x, y, width, height)` of a view (in its coordinates) in the
/// root's, through `map`, the view's transform to the root: the box
/// around its mapped corners.
fn mapped_bounds(
    map: impl Fn(f32, f32) -> Option<(f32, f32)>,
    (x, y, width, height): (f32, f32, f32, f32),
) -> Option<Bounds<Points>> {
    let corners = [
        (x, y),
        (x + width, y),
        (x, y + height),
        (x + width, y + height),
    ];
    let mapped: Vec<(f32, f32)> = corners
        .iter()
        .map(|&(x, y)| map(x, y))
        .collect::<Option<_>>()?;
    let (x0, y0) = mapped
        .iter()
        .fold((f32::MAX, f32::MAX), |(a, b), &(x, y)| (a.min(x), b.min(y)));
    let (x1, y1) = mapped
        .iter()
        .fold((f32::MIN, f32::MIN), |(a, b), &(x, y)| (a.max(x), b.max(y)));
    Some(Bounds::new(
        point(Points(x0), Points(y0)),
        Size::new(Points(x1 - x0), Points(y1 - y0)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_key_chords() {
        let (vk, modifiers) = windows_key("ctrl+a").expect("Windows key chord");
        assert_eq!(vk, VK_A);
        assert!(modifiers.ctrl);
        assert!(!modifiers.alt);
    }

    #[test]
    fn rejects_unsupported_windows_keys() {
        assert!(windows_key("not-a-windows-key").is_err());
    }

    #[cfg(feature = "dev-agent")]
    #[test]
    fn expired_attached_dispatch_skips_callback_and_disables_connection() {
        let available = AtomicBool::new(true);
        let mut called = false;
        if dispatch_is_live(&available, Instant::now() - Duration::from_millis(1)) {
            called = true;
        }
        assert!(!called);
        assert!(!available.load(Ordering::Acquire));
    }
}
