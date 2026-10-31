//! The app's keyboard shortcuts (`nsh/keymap`): the table the engine
//! mounts ([`MountItem::Keymap`](nsh_core::MountItem::Keymap)) and a local
//! key monitor that matches every key against it on the spot.
//!
//! The monitor sees a key before AppKit dispatches it: before menus' key
//! equivalents (which is why the keystrokes of Quit, Hide, Minimize and
//! Full Screen cannot be bound: `Keystroke::reserved`; the dev host's
//! Cmd+R is left alone), before the field
//! editor's key bindings and before the root's own keys (going back,
//! scrolling). A key that fires a binding is consumed (no beep, nothing
//! typed) and reported to JS with the root's `KeyCommand` event; JS runs
//! the handler afterwards. Other keys go on as if there were no monitor.
//!
//! A surface takes the keys of its window while the keyboard focus is in
//! it (its root, one of its views, a field or hosted view inside it), or
//! nowhere (the window itself).
//!
//! An app view hosted in another surface's views (an [`AppView`] inside an
//! app's native component) sees the keys of the surfaces around it, and
//! they see its keys. Which of them takes a key both bind is the hosted
//! one's [`KeyPrecedence`]: by default the focus and the monitors' order
//! decide; `First` makes it take the keys it binds wherever the focus is,
//! `Last` leaves them to the surfaces around it.
//!
//! [`AppView`]: crate::AppView

use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
    rc::Rc,
    sync::Arc,
};

use block2::RcBlock;
use nsh_core::{
    EventKind, EventPayload, PayloadValue, ROOT_ID,
    keymap::{Key, KeyMatch, Keymap, Keystroke, Modifiers},
    platform::Os,
};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSView, NSWindow};

use crate::surface::Surface;

/// A surface's table and the monitor matching keys against it (while it
/// has bindings).
#[derive(Default)]
pub(crate) struct KeymapState {
    table: RefCell<Option<Arc<Keymap>>>,
    monitor: RefCell<Option<Retained<AnyObject>>>,
    precedence: Cell<KeyPrecedence>,
}

/// Where a surface hosted in another surface's views stands for the keys
/// both bind ([`AppView::set_key_precedence`]).
///
/// [`AppView::set_key_precedence`]: crate::AppView::set_key_precedence
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyPrecedence {
    /// It takes the keys it binds while the keyboard focus is in it or
    /// nowhere, as the surfaces around it do: when both may, the newest
    /// monitor wins.
    #[default]
    Focus,
    /// It takes the keys it binds wherever the focus is in its window
    /// (while it shows); the surfaces around it get the others.
    First,
    /// The surfaces around it take the keys they bind; it gets the others.
    Last,
}

/// How `surface` stands with the surfaces around it from now on.
pub(crate) fn set_precedence(surface: &Surface, precedence: KeyPrecedence) {
    surface.keymap.precedence.set(precedence);
}

impl KeymapState {
    fn remove_monitor(&self) {
        if let Some(monitor) = self.monitor.borrow_mut().take() {
            // SAFETY: a monitor `addLocalMonitorForEventsMatchingMask`
            // returned.
            unsafe { NSEvent::removeMonitor(&monitor) };
        }
    }
}

impl Drop for KeymapState {
    fn drop(&mut self) {
        self.remove_monitor();
    }
}

/// `MountItem::Keymap`: replaces the surface's table.
pub(crate) fn set(surface: &Surface, keymap: Arc<Keymap>) {
    let state = &surface.keymap;
    if keymap.is_empty() {
        clear(surface);
        return;
    }
    *state.table.borrow_mut() = Some(keymap);
    if state.monitor.borrow().is_none() {
        let monitor = install(surface.key());
        *state.monitor.borrow_mut() = monitor;
    }
}

/// Drops the table (the app stopped).
pub(crate) fn clear(surface: &Surface) {
    surface.keymap.table.borrow_mut().take();
    surface.keymap.remove_monitor();
}

fn install(key: u64) -> Option<Retained<AnyObject>> {
    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a valid event.
        let e = unsafe { event.as_ref() };
        match Surface::with_key(key) {
            Some(surface) if pressed(&surface, e) => std::ptr::null_mut(),
            _ => event.as_ptr(),
        }
    });
    // SAFETY: the block returns the event it got, or null (consumed); the
    // monitor is removed with the table.
    unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) }
}

/// What the monitor does with key-down `event`, for a key given to a
/// window that is not the key window (`nsh test`'s, whose events the
/// monitor never sees): whether it fired a binding (then it is reported to
/// JS, and the window must not get it).
#[cfg(feature = "dev")]
pub(crate) fn fires(surface: &Rc<Surface>, event: &NSEvent) -> bool {
    pressed(surface, event)
}

/// Whether key-down `event` fired one of the surface's bindings (then it
/// is reported to JS and consumed).
fn pressed(surface: &Rc<Surface>, event: &NSEvent) -> bool {
    let Some(table) = surface.keymap.table.borrow().clone() else {
        return false;
    };
    let Some(window) = surface.root.ns().window() else {
        return false;
    };
    if event.window(surface.mtm).as_deref() != Some(&*window) {
        return false;
    }
    let Some(editing) = taking(surface, &window) else {
        return false;
    };
    if crate::input::control_activation_target(surface, event).is_some() {
        return false;
    }
    let Some(keystroke) = keystroke(event) else {
        return false;
    };
    // The dev host's Cmd+R reloads, whatever the app binds.
    if cfg!(feature = "dev") && keystroke.reloads(Os::MacOs) {
        return false;
    }
    let found = table.find(keystroke, editing, event.isARepeat());
    if found.consumed() && leaves(surface, &window, keystroke, event.isARepeat()) {
        return false;
    }
    if let KeyMatch::Fire(id) = found {
        surface.event(
            ROOT_ID,
            EventKind::KeyCommand,
            EventPayload::Value(PayloadValue::Number(id as f64)),
        );
    }
    found.consumed()
}

/// Whether the surface takes keys now, and whether they count as typed in
/// a text input ([`editing`]); a `First` surface wherever the focus is,
/// unless it is hidden.
fn taking(surface: &Surface, window: &NSWindow) -> Option<bool> {
    if surface.keymap.precedence.get() != KeyPrecedence::First {
        return editing(surface, window);
    }
    if surface.root.ns().isHiddenOrHasHiddenAncestor() {
        return None;
    }
    let focused = window.firstResponder();
    let view = focused.as_deref().and_then(|r| r.downcast_ref::<NSView>());
    Some(view.is_some_and(|v| !crate::view::is_ours(v)))
}

/// Whether `surface` leaves `keystroke`, which it binds, to another surface
/// of `window` that hosts it or is hosted in it and takes it.
fn leaves(surface: &Rc<Surface>, window: &NSWindow, keystroke: Keystroke, repeat: bool) -> bool {
    let root = surface.root.ns();
    let mine = surface.keymap.precedence.get();
    Surface::all().iter().any(|other| {
        if Rc::ptr_eq(other, surface) {
            return false;
        }
        let theirs = other.root.ns();
        let inside = theirs.isDescendantOf(root);
        if !inside && !root.isDescendantOf(theirs) {
            return false;
        }
        let precedence = other.keymap.precedence.get();
        defers(mine, precedence, inside) && binds(other, window, keystroke, repeat)
    })
}

/// Whether a surface of precedence `mine` leaves a key both bind to
/// another of precedence `theirs`, hosted in it (`inside`) or hosting it.
fn defers(mine: KeyPrecedence, theirs: KeyPrecedence, inside: bool) -> bool {
    if inside {
        theirs == KeyPrecedence::First
    } else {
        mine == KeyPrecedence::Last
    }
}

/// Whether `surface` would take `keystroke` in `window` now.
fn binds(surface: &Surface, window: &NSWindow, keystroke: Keystroke, repeat: bool) -> bool {
    let Some(table) = surface.keymap.table.borrow().clone() else {
        return false;
    };
    if surface.root.ns().window().as_deref() != Some(window) {
        return false;
    }
    taking(surface, window).is_some_and(|editing| table.find(keystroke, editing, repeat).consumed())
}

/// Where the keyboard focus is: `Some(true)` in a text input or hosted
/// view of the surface (an AppKit view of its own), `Some(false)` on one
/// of nsh's views or nowhere, `None` in another surface's views.
fn editing(surface: &Surface, window: &NSWindow) -> Option<bool> {
    let Some(responder) = window.firstResponder() else {
        return Some(false);
    };
    // The window itself: nothing has the focus.
    let Some(view) = responder.downcast_ref::<NSView>() else {
        return Some(false);
    };
    let root = surface.root.ns();
    if !view.isDescendantOf(root) {
        return None;
    }
    Some(!crate::view::is_ours(view))
}

/// The keystroke of `event`, by its key on the US layout.
fn keystroke(event: &NSEvent) -> Option<Keystroke> {
    let flags = event.modifierFlags();
    let modifiers = Modifiers {
        ctrl: flags.contains(NSEventModifierFlags::Control),
        alt: flags.contains(NSEventModifierFlags::Option),
        shift: flags.contains(NSEventModifierFlags::Shift),
        cmd: flags.contains(NSEventModifierFlags::Command),
    };
    // The virtual key codes (`kVK_*`) of keys that type no character.
    let code = event.keyCode();
    let key = match code {
        53 => Key::Escape,
        36 | 76 => Key::Enter,
        48 => Key::Tab,
        49 => Key::Char(' '),
        51 => Key::Backspace,
        117 => Key::Delete,
        // Help, where PC keyboards have Insert.
        114 => Key::Insert,
        123 => Key::Left,
        124 => Key::Right,
        125 => Key::Down,
        126 => Key::Up,
        115 => Key::Home,
        119 => Key::End,
        116 => Key::PageUp,
        121 => Key::PageDown,
        _ => match function_key(code) {
            Some(n) => Key::F(n),
            None => Key::Char(character(event, modifiers.shift)?),
        },
    };
    Some(Keystroke { modifiers, key })
}

/// The character `event`'s key types without modifiers (Shift included:
/// Shift+/ is `shift+/`), in lower case; on a layout that types no ASCII
/// there, the US layout's character of the key, as menus match it.
fn character(event: &NSEvent, shift: bool) -> Option<char> {
    let typed = if shift {
        event.charactersByApplyingModifiers(NSEventModifierFlags::empty())
    } else {
        event.charactersIgnoringModifiers()
    };
    match typed.and_then(|s| s.to_string().chars().next()) {
        Some(c) if c.is_ascii_graphic() => Some(c.to_ascii_lowercase()),
        _ => crate::input::us_key(event.keyCode()),
    }
}

/// F1 to F20, by virtual key code.
fn function_key(code: u16) -> Option<u8> {
    const KEYS: [u16; 20] = [
        122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90,
    ];
    KEYS.iter().position(|&k| k == code).map(|i| i as u8 + 1)
}

#[cfg(test)]
mod tests {
    use super::{KeyPrecedence, defers};

    #[test]
    fn a_hosted_surface_first_takes_the_keys_both_bind() {
        // the host leaves them to it; it keeps them from the host
        assert!(defers(KeyPrecedence::Focus, KeyPrecedence::First, true));
        assert!(!defers(KeyPrecedence::First, KeyPrecedence::Focus, false));
    }

    #[test]
    fn a_hosted_surface_last_leaves_them_to_its_host() {
        assert!(defers(KeyPrecedence::Last, KeyPrecedence::Focus, false));
        assert!(!defers(KeyPrecedence::Focus, KeyPrecedence::Last, true));
    }

    #[test]
    fn by_default_neither_side_leaves_a_key() {
        assert!(!defers(KeyPrecedence::Focus, KeyPrecedence::Focus, true));
        assert!(!defers(KeyPrecedence::Focus, KeyPrecedence::Focus, false));
    }

    #[test]
    fn function_keys() {
        assert_eq!(super::function_key(122), Some(1));
        assert_eq!(super::function_key(96), Some(5));
        assert_eq!(super::function_key(90), Some(20));
        assert_eq!(super::function_key(0), None);
    }
}
