//! Whose hands the app's input came from: the person's, or an automation's.
//!
//! The app registers one listener ([`on_input`]) and hears
//! [`Input::Person`] for each press or key given to a surface, and
//! [`Input::Agent`] while an automation acts. What it does with them is
//! its own business; nsh only says where input came from.
//!
//! Input made in process ([`crate::synth`]) is told apart from the
//! system's by a mark on the event, so a synthesized press never reads as
//! the person's. Nor does an event another process posted: the system says
//! which process that was, and hardware is no process ([`source_of`]). Hover
//! and scroll say nothing: they come without the person meaning to act.

use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
    rc::Rc,
};

use block2::RcBlock;
use objc2::{MainThreadMarker, Message, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType, NSView};
use objc2_core_graphics::{CGEvent, CGEventField};

use crate::view::{NshView, superview};

/// Input the app's listener hears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// An automation is acting, on any surface: a menu item it fires has
    /// no surface, and what it presses may open another.
    Agent,
    /// The person pressed or typed in a surface.
    Person {
        /// Its key ([`crate::AppView::surface_key`]).
        surface: u64,
    },
}

/// The `eventNumber` of the mouse events [`crate::synth`] makes. A real
/// event that happens to carry it counts as synthesized: the safe way round.
pub(crate) const SYNTH_EVENT: isize = 0x6e_73_68;

type Listener = Rc<dyn Fn(Input)>;

thread_local! {
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    /// A key event [`crate::synth`] is handing over now (key events carry
    /// no number to mark).
    static SYNTH_KEY: Cell<bool> = const { Cell::new(false) };
    /// Whether synthesized input is the developer's, not an agent's.
    static SYNTH_IS_PERSON: Cell<bool> = const { Cell::new(false) };
    /// The in-process actions running ([`crate::AppView::act`]): theirs
    /// are an agent's whatever the test channel says.
    static AGENT_RUNS: Cell<u32> = const { Cell::new(0) };
}

/// Sets the listener and starts watching the app's input. Main thread.
pub fn on_input(mtm: MainThreadMarker, listener: impl Fn(Input) + 'static) {
    LISTENER.set(Some(Rc::new(listener)));
    if MONITOR.with_borrow(Option::is_some) {
        return;
    }
    // Monitors belong to the application; this may run before it exists.
    let _ = objc2_app_kit::NSApplication::sharedApplication(mtm);
    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a valid event.
        observe(unsafe { event.as_ref() }, mtm);
        event.as_ptr()
    });
    let mask = NSEventMask::LeftMouseDown
        | NSEventMask::RightMouseDown
        | NSEventMask::OtherMouseDown
        | NSEventMask::KeyDown;
    // SAFETY: the block hands the event back unchanged; the monitor lives
    // as long as the app.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    MONITOR.set(monitor);
}

fn tell(input: Input) {
    let listener = LISTENER.with_borrow(Clone::clone);
    if let Some(listener) = listener {
        listener(input);
    }
}

/// An automation is acting: called for each of its actions while it runs.
/// Silent when synthesized input is the developer's.
#[cfg(feature = "gpui-agent")]
#[cfg_attr(not(feature = "gpui-agent-server"), allow(dead_code))]
pub(crate) fn agent_acts() {
    if !SYNTH_IS_PERSON.get() {
        tell(Input::Agent);
    }
}

/// An in-process action began (`true`) or ended.
#[cfg(feature = "gpui-agent")]
pub(crate) fn agent_run(started: bool) {
    AGENT_RUNS.set(if started {
        AGENT_RUNS.get() + 1
    } else {
        AGENT_RUNS.get().saturating_sub(1)
    });
}

/// Whether an in-process action is running.
#[cfg(all(test, feature = "gpui-agent"))]
pub(crate) fn agent_running() -> bool {
    AGENT_RUNS.get() > 0
}

/// An in-process action is stepping: always an agent's.
#[cfg(feature = "gpui-agent")]
pub(crate) fn agent_step() {
    tell(Input::Agent);
}

/// Makes synthesized input the developer's: it reads as the person's, as
/// a test channel's always did.
#[cfg(feature = "gpui-agent-server")]
pub(crate) fn synth_is_person() {
    SYNTH_IS_PERSON.set(true);
}

/// Runs `f`, which hands a synthesized key event to the app.
#[cfg(any(feature = "dev", feature = "gpui-agent"))]
pub(crate) fn synth_key(f: impl FnOnce()) {
    let old = SYNTH_KEY.replace(true);
    f();
    SYNTH_KEY.set(old);
}

fn synthesized(kind: NSEventType, number: impl FnOnce() -> isize) -> bool {
    // `eventNumber` is a mouse event's only: AppKit raises on a key event.
    if kind == NSEventType::KeyDown {
        SYNTH_KEY.get()
    } else {
        number() == SYNTH_EVENT
    }
}

/// Whether synthesized input may read as the person's: the developer's
/// channel's may, while no in-process action is running.
fn counts_as_person() -> bool {
    SYNTH_IS_PERSON.get() && AGENT_RUNS.get() == 0
}

/// The process that posted `event`: 0 for hardware, `None` when it can't be
/// told.
fn source_of(event: &NSEvent) -> Option<i64> {
    let event = event.CGEvent()?;
    Some(CGEvent::integer_value_field(
        Some(&event),
        CGEventField::EventSourceUnixProcessID,
    ))
}

/// Whether input is the person's: what this process made, by
/// [`counts_as_person`]; anything else only with no process behind it.
fn persons(synthesized: bool, source: impl FnOnce() -> Option<i64>) -> bool {
    if synthesized {
        counts_as_person()
    } else {
        source() == Some(0)
    }
}

fn observe(event: &NSEvent, mtm: MainThreadMarker) {
    let kind = event.r#type();
    if !persons(synthesized(kind, || event.eventNumber()), || {
        source_of(event)
    }) {
        return;
    }
    let target = if kind == NSEventType::KeyDown {
        key_target(event, mtm)
    } else {
        press_target(event, mtm)
    };
    if let Some(surface) = target.as_deref().and_then(surface_of) {
        tell(Input::Person { surface });
    }
}

/// The view a press lands on, by the hit test the window delivers it with.
fn press_target(event: &NSEvent, mtm: MainThreadMarker) -> Option<Retained<NSView>> {
    let content = event.window(mtm)?.contentView()?;
    let p = event.locationInWindow();
    let in_super = superview(&content).map_or(p, |frame| frame.convertPoint_fromView(p, None));
    content.hitTest(in_super)
}

/// The view a key goes to.
fn key_target(event: &NSEvent, mtm: MainThreadMarker) -> Option<Retained<NSView>> {
    event
        .window(mtm)?
        .firstResponder()?
        .downcast::<NSView>()
        .ok()
}

/// The key of the innermost surface `view` is in.
fn surface_of(view: &NSView) -> Option<u64> {
    let mut current = Some(view.retain());
    while let Some(v) = current {
        if let Some(surface) = NshView::from_ns(&v).and_then(|n| n.surface()) {
            return Some(surface.key());
        }
        current = superview(&v);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_marked_event_is_synthesized() {
        assert!(synthesized(NSEventType::LeftMouseDown, || SYNTH_EVENT));
        assert!(!synthesized(NSEventType::LeftMouseDown, || 7));
        assert!(!synthesized(NSEventType::KeyDown, || unreachable!(
            "keys have no number"
        )));
        SYNTH_KEY.set(true);
        assert!(synthesized(NSEventType::KeyDown, || unreachable!(
            "keys have no number"
        )));
        SYNTH_KEY.set(false);
    }

    #[test]
    fn an_event_a_process_posted_is_not_the_persons() {
        assert!(persons(false, || Some(0)), "hardware");
        assert!(!persons(false, || Some(17897)), "another process");
        assert!(!persons(false, || None), "can't tell is no");
    }

    /// The test channel's events come from this very process: its word is
    /// the channel's setting, and their source is never looked at.
    #[test]
    fn the_test_channel_keeps_its_own_rule() {
        assert!(!persons(true, || unreachable!(
            "synthesized goes by the channel"
        )));
        SYNTH_IS_PERSON.set(true);
        assert!(persons(true, || unreachable!(
            "synthesized goes by the channel"
        )));
        SYNTH_IS_PERSON.set(false);
    }

    #[test]
    fn an_in_process_action_is_never_the_persons_whatever_the_channel_says() {
        assert!(!counts_as_person());
        SYNTH_IS_PERSON.set(true);
        assert!(counts_as_person());
        AGENT_RUNS.set(1);
        assert!(!counts_as_person());
        AGENT_RUNS.set(0);
        SYNTH_IS_PERSON.set(false);
    }
}
