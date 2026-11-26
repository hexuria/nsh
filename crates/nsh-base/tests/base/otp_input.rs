//! `OtpInput`: typing, Backspace, paste and the length on the native field
//! over its cells, digits only, the code the app owns or its own, the
//! events, masked cells, the disabled state and the filled cells'
//! `data-filled`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use nsh_base::{OtpInputCompleteEvent, input, otp_input};
use nsh_core::{
    AutoFill, Color, Command, EventKind, EventPayload, MountItem, NodeId, PasteEvent, PayloadValue,
    Size, element::Root, prelude::*,
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

/// The inputs made: their ids, whether each is secure, and whether it is
/// a one-time code's field.
type Inputs = Arc<Mutex<Vec<(NodeId, bool, bool)>>>;

fn start<C: Component<Props = ()>>() -> Headless {
    start_observing::<C>().0
}

/// [`start`], with the inputs made: whether each is secure, and whether
/// it is a one-time code's field.
fn start_observing<C: Component<Props = ()>>() -> (Headless, Inputs) {
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<C>())
    })
    .unwrap();
    let inputs: Inputs = Arc::new(Mutex::new(Vec::new()));
    let seen = inputs.clone();
    app.observe(move |_, batch| {
        let mut seen = seen.lock().unwrap();
        for item in &batch.items {
            if let MountItem::Input { id, config } = item {
                let code = config.auto_fill == AutoFill::OneTimeCode;
                match seen.iter_mut().find(|(i, ..)| i == id) {
                    Some(input) => (input.1, input.2) = (config.secure, code),
                    None => seen.push((*id, config.secure, code)),
                }
            }
        }
    });
    app.first_render().unwrap();
    (app, inputs)
}

/// The cells' size: an `OtpInput` has no look of its own, so without one
/// its cells, and the field over them, have no size, and the screen (as a
/// user) finds nothing to type into.
fn sized(input: nsh_base::OtpInputElement) -> nsh_base::OtpInputElement {
    input
        .cell_style(|s| s.w(px(30.0)).h(px(40.0)))
        .group_class("flex-row")
}

/// What the app heard, as texts on screen.
#[derive(Clone, Copy)]
struct Log {
    changes: Signal<Vec<String>>,
    completes: Signal<Vec<String>>,
    focus: Signal<(u32, u32)>,
}

impl Log {
    fn new() -> Self {
        Self {
            changes: signal(Vec::new()),
            completes: signal(Vec::new()),
            focus: signal((0, 0)),
        }
    }

    fn view(self) -> impl IntoElement {
        view()
            .child(text(move || {
                format!("Changes [{}]", self.changes.get().join(","))
            }))
            .child(text(move || {
                format!("Completes [{}]", self.completes.get().join(","))
            }))
            .child(text(move || {
                let (focus, blur) = self.focus.get();
                format!("Focus {focus} Blur {blur}")
            }))
    }

    fn change(self, value: &SharedString) {
        self.changes.update(|c| c.push(value.to_string()));
    }

    fn complete(self, event: &OtpInputCompleteEvent) {
        self.completes.update(|c| c.push(event.value.to_string()));
    }
}

/// Six digits of its own, a field to move the focus to.
#[component]
fn Uncontrolled() -> impl IntoElement {
    let log = Log::new();
    view()
        .child(
            view().test_id("otp").child(
                sized(otp_input())
                    .length(6u32)
                    .accessibility_label("Code")
                    .on_change(move |v: &SharedString| log.change(v))
                    .on_complete(move |e: &OtpInputCompleteEvent| log.complete(e))
                    .on_focus(move || log.focus.update(|f| f.0 += 1))
                    .on_blur(move || log.focus.update(|f| f.1 += 1)),
            ),
        )
        .child(view().test_id("other").child(input()))
        .child(log.view())
}

/// The code the app owns, kept when `locked`; a default value.
#[component]
fn Controlled() -> impl IntoElement {
    let log = Log::new();
    let code = signal(SharedString::from("12"));
    let locked = signal(false);
    view()
        .child(
            view().test_id("otp").child(
                sized(otp_input())
                    .length(4u32)
                    .groups(2u32)
                    .value(move || Some(code.get()))
                    .on_change(move |v: &SharedString| {
                        log.change(v);
                        if !locked.get_untracked() {
                            code.set(v.clone());
                        }
                    }),
            ),
        )
        .child(
            view()
                .test_id("defaulted")
                .child(sized(otp_input()).length(4u32).default_value("98")),
        )
        .child(
            view()
                .test_id("clear")
                .on_press(move || code.set(SharedString::default()))
                .child(text("Clear")),
        )
        .child(
            view()
                .test_id("lock")
                .on_press(move || locked.set(true))
                .child(text("Lock")),
        )
        .child(text(move || format!("Code [{}]", code.get())))
        .child(log.view())
}

/// The code the app owns, set a turn after `onChange`, as a TSX app's
/// comes back.
#[component]
fn Late() -> impl IntoElement {
    let code = signal(SharedString::default());
    view()
        .child(
            view().test_id("otp").child(
                sized(otp_input())
                    .length(6u32)
                    .value(move || Some(code.get()))
                    .on_change(move |v: &SharedString| {
                        let v = v.clone();
                        set_timeout(Duration::from_millis(1), move || code.set(v));
                    }),
            ),
        )
        .child(text(move || format!("Code [{}]", code.get())))
}

/// Masked and disabled inputs.
#[component]
fn States() -> impl IntoElement {
    view()
        .child(
            view().test_id("masked").child(
                sized(otp_input())
                    .length(4u32)
                    .masked(true)
                    .accessibility_label("Masked"),
            ),
        )
        .child(
            view().test_id("disabled").child(
                sized(otp_input())
                    .length(4u32)
                    .disabled(true)
                    .default_value("12")
                    .accessibility_label("Disabled"),
            ),
        )
}

/// The color [`Filled`]'s `data-filled:` class gives a cell.
const FILLED: Color = Color(0x0101_01ff);

/// Codes whose filled cells take a background from `data-filled:`, one
/// masked.
#[component]
fn Filled() -> impl IntoElement {
    view()
        .child(
            view().test_id("otp").child(
                sized(otp_input())
                    .length(4u32)
                    .groups(2u32)
                    .default_value("1")
                    .cell_class("data-filled:bg-[#010101]")
                    .accessibility_label("Code"),
            ),
        )
        .child(
            view().test_id("masked").child(
                sized(otp_input())
                    .length(3u32)
                    .masked(true)
                    .default_value("12")
                    .cell_class("data-filled:bg-[#010101]")
                    .accessibility_label("Masked"),
            ),
        )
}

/// [`Filled`], and how many cells the latest styles fill.
fn start_filled() -> (Headless, Arc<Mutex<HashMap<NodeId, bool>>>) {
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<Filled>())
    })
    .unwrap();
    let styles = Arc::new(Mutex::new(HashMap::new()));
    let seen = styles.clone();
    app.observe(move |_, batch| {
        let mut seen = seen.lock().unwrap();
        for item in &batch.items {
            match item {
                MountItem::ViewStyle { id, style } => {
                    seen.insert(*id, style.background_color == FILLED);
                }
                MountItem::Delete { id } => {
                    seen.remove(id);
                }
                _ => {}
            }
        }
    });
    app.first_render().unwrap();
    (app, styles)
}

fn filled_cells(styles: &Mutex<HashMap<NodeId, bool>>) -> usize {
    styles.lock().unwrap().values().filter(|f| **f).count()
}

fn see(app: &mut Headless, text: &str) {
    app.wait_for_within(text, Duration::from_secs(2)).unwrap();
}

/// The field's text, once the input took back what it dropped.
fn field(app: &mut Headless, selector: &str) -> String {
    app.wait(Duration::from_millis(50)).unwrap();
    app.screen()
        .input_text(selector)
        .unwrap()
        .expect("a field")
        .to_owned()
}

#[test]
fn typing_digits_fills_the_cells_and_reports_each_change() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    see(&mut app, "Changes [1,12,123]");
    for digit in ["1", "2", "3"] {
        assert!(app.shows(digit), "{digit}: {:?}", app.screen().texts());
    }
    assert_eq!(field(&mut app, "id:otp"), "123");
    see(&mut app, "Completes []");
}

#[test]
fn other_characters_are_dropped_and_the_field_keeps_the_code() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "1a2-").unwrap();
    see(&mut app, "Changes [1,12]");
    assert_eq!(field(&mut app, "id:otp"), "12");
    assert!(!app.shows("a"));
}

#[test]
fn full_width_digits_are_taken_as_ascii_digits() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "１２").unwrap();
    see(&mut app, "Changes [1,12]");
    assert_eq!(field(&mut app, "id:otp"), "12");
}

#[test]
fn backspace_removes_the_last_digit() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    app.press_key("backspace").unwrap();
    see(&mut app, "Changes [1,12,123,12]");
    assert!(!app.shows("3"));
    assert_eq!(field(&mut app, "id:otp"), "12");
}

#[test]
fn the_length_caps_the_code_and_completes_it_once() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "1234567").unwrap();
    see(&mut app, "Changes [1,12,123,1234,12345,123456]");
    see(&mut app, "Completes [123456]");
    assert_eq!(field(&mut app, "id:otp"), "123456");
    // More keys change nothing and complete nothing.
    app.type_text("id:otp", "8").unwrap();
    see(&mut app, "Completes [123456]");
    assert_eq!(field(&mut app, "id:otp"), "123456");
}

#[test]
fn a_pasted_code_replaces_the_code_and_shorter_text_is_appended() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "1").unwrap();
    app.paste("id:otp", &PasteEvent::new("98-76-54-32"))
        .unwrap();
    see(&mut app, "Changes [1,987654]");
    see(&mut app, "Completes [987654]");
    assert_eq!(field(&mut app, "id:otp"), "987654");

    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "12").unwrap();
    app.paste("id:otp", &PasteEvent::new(" 3 4")).unwrap();
    see(&mut app, "Changes [1,12,1234]");
    assert_eq!(field(&mut app, "id:otp"), "1234");
}

#[test]
fn the_focus_is_reported_as_it_comes_and_goes() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "").unwrap();
    see(&mut app, "Focus 1 Blur 0");
    app.type_text("id:other", "x").unwrap();
    see(&mut app, "Focus 1 Blur 1");
}

#[test]
fn a_caret_inside_the_text_is_the_users_own() {
    let (mut app, inputs) = start_observing::<Uncontrolled>();
    app.type_text("id:otp", "12").unwrap();
    see(&mut app, "Changes [1,12]");
    // A caret inside the text (an arrow key, a click, the start of a
    // drag) is not pinned to the end: pinning it was the burst fault, and
    // it undid the caret moves a selection starts with. The reconstruction
    // reads an edit wherever it happens, so the end needs no enforcing.
    let field = inputs
        .lock()
        .unwrap()
        .iter()
        .find(|i| i.2)
        .expect("the field")
        .0;
    let at = PayloadValue::Number;
    let before = app.screen().commands().len();
    app.engine().control().dispatch_event(
        field,
        EventKind::SelectionChange,
        EventPayload::Map(vec![("start", at(1.0)), ("end", at(1.0))]),
    );
    app.wait(Duration::from_millis(50)).unwrap();
    assert!(
        !app.screen().commands()[before..]
            .iter()
            .any(|(id, c)| *id == field && matches!(c, Command::SetSelection { .. })),
        "no pin answers a caret: {:?}",
        &app.screen().commands()[before..]
    );
}

#[test]
fn the_apps_code_is_shown_and_only_its_changes_reported() {
    let mut app = start::<Controlled>();
    see(&mut app, "Code [12]");
    assert!(app.shows("1") && app.shows("2"));
    assert_eq!(field(&mut app, "id:otp"), "12");
    // The app takes the user's change.
    app.type_text("id:otp", "3").unwrap();
    see(&mut app, "Code [123]");
    see(&mut app, "Changes [123]");
    // A code the app sets is shown and put in the field, not reported.
    app.press("id:clear").unwrap();
    see(&mut app, "Code []");
    assert!(!app.shows("1"));
    assert_eq!(field(&mut app, "id:otp"), "");
    see(&mut app, "Changes [123]");
    // A change the app keeps out is reported, and the cells keep its code.
    app.press("id:lock").unwrap();
    app.type_text("id:otp", "7").unwrap();
    see(&mut app, "Changes [123,7]");
    see(&mut app, "Code []");
    assert!(!app.shows("7"));
}

#[test]
fn keys_typed_before_the_apps_code_comes_back_build_on_each_other() {
    let mut app = start::<Late>();
    app.type_text("id:otp", "135").unwrap();
    see(&mut app, "Code [135]");
    assert_eq!(field(&mut app, "id:otp"), "135");
    for digit in ["1", "3", "5"] {
        assert!(app.shows(digit), "{digit}: {:?}", app.screen().texts());
    }
    app.press_key("backspace").unwrap();
    see(&mut app, "Code [13]");
    assert_eq!(field(&mut app, "id:otp"), "13");
}

#[test]
fn a_default_value_is_the_first_code() {
    let mut app = start::<Controlled>();
    assert!(app.shows("9") && app.shows("8"));
    assert_eq!(field(&mut app, "id:defaulted"), "98");
    app.type_text("id:defaulted", "7").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    assert!(app.shows("7"));
    assert_eq!(field(&mut app, "id:defaulted"), "987");
}

#[test]
fn masked_cells_show_bullets_and_the_field_is_secure() {
    let (mut app, inputs) = start_observing::<States>();
    // The masked field is secure (screen readers do not read it), the
    // other is not.
    let mut secure: Vec<bool> = inputs.lock().unwrap().iter().map(|i| i.1).collect();
    secure.sort();
    assert_eq!(secure, [false, true], "{:?}", inputs.lock().unwrap());
    app.type_text("id:masked", "34").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    let bullets = app
        .screen()
        .texts()
        .iter()
        .filter(|t| t.trim() == "\u{2022}")
        .count();
    assert_eq!(bullets, 2, "{:?}", app.screen().texts());
    assert!(!app.shows("3") && !app.shows("4"));
    assert_eq!(field(&mut app, "id:masked"), "34");
    // The disabled input's code is not masked.
    assert!(app.shows("1") && app.shows("2"));
}

#[test]
fn a_disabled_input_takes_no_input() {
    let app = start::<States>();
    assert!(!app.screen().enabled("label:Disabled").unwrap());
    assert!(app.screen().enabled("label:Masked").unwrap());
}

#[test]
fn the_caret_blinks_without_mount_items() {
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        clock: nsh_engine::Clock::manual(std::time::SystemTime::now()),
        ..EngineConfig::new(Root::of::<Uncontrolled>())
    })
    .unwrap();
    let items = Arc::new(Mutex::new(0usize));
    let seen = items.clone();
    app.observe(move |_, batch| *seen.lock().unwrap() += batch.items.len());
    app.first_render().unwrap();
    app.type_text("id:otp", "1").unwrap();
    see(&mut app, "Changes [1]");
    app.run_for(Duration::from_millis(100)).unwrap();
    *items.lock().unwrap() = 0;
    // Four blinks: the caret's opacity moves natively, the engine sends
    // nothing.
    app.run_for(Duration::from_secs(2)).unwrap();
    assert_eq!(
        *items.lock().unwrap(),
        0,
        "mount items while the caret blinks"
    );
}

#[test]
fn filled_cells_take_their_data_filled_classes() {
    let (mut app, styles) = start_filled();
    // The code's first digit and the masked code's two (bullets, filled
    // all the same).
    assert_eq!(filled_cells(&styles), 3);
    app.type_text("id:otp", "23").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "123");
    assert_eq!(filled_cells(&styles), 5);
    // Backspace empties the last cell, which loses the class.
    app.press_key("backspace").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "12");
    assert_eq!(filled_cells(&styles), 4);
}

/// Nine digits in one burst, as the OS delivers fast typing: several in
/// flight while the first keys' roundtrip lands mid-burst. Before the
/// caret fix the pins the lagging binding caused pulled the caret back
/// into the text and the rest of the keys landed in the middle of it.
#[test]
fn a_burst_of_keys_keeps_its_order() {
    let (mut app, inputs) = start_observing::<Uncontrolled>();
    let field = inputs
        .lock()
        .unwrap()
        .iter()
        .find(|i| i.2)
        .expect("the field")
        .0;
    let _ = field;
    app.type_in_burst("id:otp", "123456", 2).unwrap();
    see(&mut app, "Changes [1,12,123,1234,12345,123456]");
}

/// A key landing while the field is being rewritten is not dropped: the
/// invalid character makes the input rewrite the field mid-burst, and the
/// digits after it must all survive in order.
#[test]
fn a_key_during_the_rewrite_is_not_dropped() {
    let mut app = start::<Uncontrolled>();
    app.type_in_burst("id:otp", "1a23456", 3).unwrap();
    see(&mut app, "Changes [1,12,123,1234,12345,123456]");
}

/// The keyboard changing layout or an input method restarting mid-entry —
/// digits arriving around a composition — loses nothing and keeps the
/// order (the composition commits nothing here, as a relayout's does).
#[test]
fn a_composition_mid_entry_drops_no_key() {
    let mut app = start::<Uncontrolled>();
    app.type_in_burst("id:otp", "123", 2).unwrap();
    app.compose("id:otp", "").unwrap();
    app.type_in_burst("id:otp", "456", 1).unwrap();
    see(&mut app, "Changes [1,12,123,1234,12345,123456]");
}

/// A selection the user makes (a long press and drag, for the edit menu)
/// is not pinned back to the end: only a bare caret is, so the menu's
/// Paste has its selection to work on.
#[test]
fn a_users_selection_is_not_pinned_away() {
    let (mut app, inputs) = start_observing::<Uncontrolled>();
    app.type_text("id:otp", "12").unwrap();
    see(&mut app, "Changes [1,12]");
    let field = inputs
        .lock()
        .unwrap()
        .iter()
        .find(|i| i.2)
        .expect("the field")
        .0;
    let before = app.screen().commands().len();
    let at = PayloadValue::Number;
    app.engine().control().dispatch_event(
        field,
        EventKind::SelectionChange,
        EventPayload::Map(vec![("start", at(0.0)), ("end", at(1.0))]),
    );
    app.wait(Duration::from_millis(50)).unwrap();
    assert!(
        app.screen().commands().len() == before
            || !app.screen().commands()[before..]
                .iter()
                .any(|(id, c)| *id == field && matches!(c, Command::SetSelection { .. })),
        "no pin answers a selection: {:?}",
        &app.screen().commands()[before..]
    );
}

/// Three digits in, a selection removed with Backspace (what the edit
/// menu's Cut and the delete keys share: the range goes, the rest close
/// up): one digit first, middle or last, two, or all of them.
#[test]
fn a_selection_cut_removes_exactly_it() {
    for (from, end, rest) in [
        (0, 1, "23"),
        (1, 2, "13"),
        (2, 3, "12"),
        (0, 2, "3"),
        (1, 3, "1"),
        (0, 3, ""),
    ] {
        let mut app = start::<Uncontrolled>();
        app.type_text("id:otp", "123").unwrap();
        see(&mut app, "Changes [1,12,123]");
        app.select("id:otp", from, end).unwrap();
        app.press_key("backspace").unwrap();
        app.wait(Duration::from_millis(50)).unwrap();
        assert_eq!(field(&mut app, "id:otp"), rest, "cut ({from},{end}) of 123");
    }
}

/// The forward delete key over a selection removes exactly it too.
#[test]
fn a_selection_forward_delete_removes_exactly_it() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    app.select("id:otp", 1, 2).unwrap();
    app.press_key("delete").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "13");
}

/// Typing over a selection replaces exactly it, the rest untouched.
#[test]
fn typing_over_a_selection_replaces_exactly_it() {
    for (from, end, code) in [(1, 2, "153"), (0, 1, "523"), (2, 3, "125"), (0, 3, "5")] {
        let mut app = start::<Uncontrolled>();
        app.type_text("id:otp", "123").unwrap();
        app.select("id:otp", from, end).unwrap();
        app.type_text("id:otp", "5").unwrap();
        app.wait(Duration::from_millis(50)).unwrap();
        assert_eq!(
            field(&mut app, "id:otp"),
            code,
            "type 5 over ({from},{end}) of 123"
        );
    }
}

/// Backspace with no selection removes the digit before the caret: at
/// the end, the last cell's.
#[test]
fn a_bare_backspace_removes_the_digit_before_the_caret() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    app.press_key("backspace").unwrap();
    app.wait(Duration::from_millis(50)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "12");
}

/// The controlled matrix: the same cut, delete and type-over cases with
/// the app owning the code, as every real use has it.
#[test]
fn a_controlled_selection_cut_removes_exactly_it() {
    for (from, end, rest) in [(0, 1, "23"), (1, 2, "13"), (2, 3, "12"), (0, 3, "")] {
        let mut app = start::<Controlled>();
        app.clear("id:otp").unwrap();
        app.type_text("id:otp", "123").unwrap();
        see(&mut app, "Code [123]");
        app.select("id:otp", from, end).unwrap();
        app.press_key("backspace").unwrap();
        app.wait(Duration::from_millis(80)).unwrap();
        assert_eq!(
            field(&mut app, "id:otp"),
            rest,
            "controlled cut ({from},{end}) of 123"
        );
    }
}

/// The controlled roundtrip a turn late, as a TSX app's comes back: a
/// cut still removes exactly the selection.
#[test]
fn a_late_controlled_selection_cut_removes_exactly_it() {
    for (from, end, rest) in [(0, 1, "23"), (1, 2, "13"), (2, 3, "12"), (0, 3, "")] {
        let mut app = start::<Late>();
        app.type_text("id:otp", "123").unwrap();
        app.wait(Duration::from_millis(150)).unwrap();
        app.select("id:otp", from, end).unwrap();
        app.press_key("backspace").unwrap();
        app.wait(Duration::from_millis(150)).unwrap();
        assert_eq!(
            field(&mut app, "id:otp"),
            rest,
            "late cut ({from},{end}) of 123"
        );
    }
}

/// Typing over a selection in the controlled input, the echo a turn late.
#[test]
fn a_late_controlled_type_over_replaces_exactly_it() {
    let mut app = start::<Late>();
    app.type_text("id:otp", "123").unwrap();
    app.wait(Duration::from_millis(150)).unwrap();
    app.select("id:otp", 1, 2).unwrap();
    app.type_text("id:otp", "5").unwrap();
    app.wait(Duration::from_millis(150)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "153");
}

/// The keyboard path to a selection, as a user forms it: arrows move the
/// caret, shifted arrows grow the selection. A caret inside the text must
/// stay where the arrow put it — before the fix the pin yanked it back to
/// the end, so a selection meant for the first or middle digit formed at
/// the end and the cut or delete removed the wrong digits.
#[test]
fn an_arrowed_selection_cuts_exactly_it() {
    // (arrows left from the end, shifted left, the digits of 123 that stay)
    for (lefts, shifted, rest) in [
        (2, 1, "23"), // first digit
        (1, 1, "13"), // middle
        (0, 1, "12"), // last
        (1, 2, "3"),  // first two
        (0, 2, "1"),  // last two
    ] {
        let mut app = start::<Uncontrolled>();
        app.type_text("id:otp", "123").unwrap();
        see(&mut app, "Changes [1,12,123]");
        for _ in 0..lefts {
            app.press_key("left").unwrap();
        }
        for _ in 0..shifted {
            app.press_key("shift+left").unwrap();
        }
        app.press_key("backspace").unwrap();
        app.wait(Duration::from_millis(80)).unwrap();
        assert_eq!(
            field(&mut app, "id:otp"),
            rest,
            "left {lefts} shift+left {shifted} of 123"
        );
    }
}

/// Typing over an arrowed selection replaces exactly it.
#[test]
fn typing_over_an_arrowed_selection_replaces_exactly_it() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    see(&mut app, "Changes [1,12,123]");
    app.press_key("left").unwrap();
    app.press_key("shift+left").unwrap();
    app.type_text("id:otp", "5").unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "153");
}

/// A bare backspace after arrows removes the digit before the caret, not
/// the last digit.
#[test]
fn a_bare_backspace_after_arrows_removes_the_digit_before_the_caret() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    see(&mut app, "Changes [1,12,123]");
    app.press_key("left").unwrap();
    app.press_key("backspace").unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "13");
}

/// A tap on a cell places the caret where it lands: the left half of a
/// cell before its digit, the right half after it, a tap past the last
/// cell at the end. The field's own placement cannot do this — its glyphs
/// are one continuous run with no relation to the cells — so the input
/// corrects it from the cells' frames.
#[test]
fn a_tap_on_a_cell_places_the_caret_there() {
    // (tap x in the input, caret; the cells are 30 wide in a row: cell 0
    // is x 0-30, cell 1 x 30-60, cell 2 x 60-90)
    for (x, caret) in [
        (5.0, 0),  // left half of cell 0
        (25.0, 1), // right half of cell 0
        (35.0, 1), // left half of cell 1
        (55.0, 2), // right half of cell 1
        (65.0, 2), // left half of cell 2
        (85.0, 3), // right half of cell 2
    ] {
        let mut app = start::<Uncontrolled>();
        app.type_text("id:otp", "123").unwrap();
        app.press_at("label:Code", Point::new(px(x), px(20.0)))
            .unwrap();
        app.wait(Duration::from_millis(80)).unwrap();
        assert_eq!(
            app.screen().input_selection("id:otp").unwrap(),
            Some((caret, caret)),
            "tap at x {x} of three 30-wide cells"
        );
    }
}

/// A tap's caret is where Backspace and typing then act: caret after the
/// second digit, one Backspace removes the second digit, not the last.
#[test]
fn a_backspace_at_a_tapped_caret_removes_the_digit_before_it() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "123").unwrap();
    see(&mut app, "Changes [1,12,123]");
    // Right half of cell 1: the caret goes after digit 2.
    app.press_at("label:Code", Point::new(px(55.0), px(20.0)))
        .unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    app.press_key("backspace").unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "13");
}

/// Typing at a tapped caret inserts there: caret after the first digit,
/// a digit goes in before the second.
#[test]
fn typing_at_a_tapped_caret_inserts_there() {
    let mut app = start::<Uncontrolled>();
    app.type_text("id:otp", "12").unwrap();
    app.press_at("label:Code", Point::new(px(25.0), px(20.0)))
        .unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    app.type_text("id:otp", "5").unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(field(&mut app, "id:otp"), "152");
}

/// A selection the platform reports is drawn on exactly the cells it
/// covers: the middle digit alone, or every cell for the whole run.
#[test]
fn a_reported_selection_is_drawn_on_the_right_cells() {
    let (mut app, styles) = start_selected();
    app.type_text("id:otp", "123").unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    // No selection: nothing drawn.
    assert_eq!(marked_cells(&styles), 0, "no selection draws nothing");
    app.select("id:otp", 1, 2).unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(marked_cells(&styles), 1, "one digit selected");
    app.select("id:otp", 0, 3).unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(marked_cells(&styles), 3, "the whole run selected");
    app.select("id:otp", 2, 2).unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    assert_eq!(marked_cells(&styles), 0, "a caret draws nothing");
}

/// The highlight lands on the right cell, not just the right number of
/// cells: a selection of the first digit marks a cell left of one a
/// selection of the last digit marks.
#[test]
fn the_highlight_is_on_the_selected_cell() {
    let (mut app, styles) = start_selected();
    app.type_text("id:otp", "123").unwrap();
    app.select("id:otp", 0, 1).unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    let first = marked_positions(&app, &styles);
    app.select("id:otp", 2, 3).unwrap();
    app.wait(Duration::from_millis(80)).unwrap();
    let last = marked_positions(&app, &styles);
    assert_eq!(first.len(), 1, "one cell for the first digit: {first:?}");
    assert_eq!(last.len(), 1, "one cell for the last digit: {last:?}");
    assert!(
        first[0] < last[0],
        "the first digit's cell is left of the last's: {first:?} vs {last:?}"
    );
}

/// The color [`Selected`]'s `data-selected:` class gives a cell.
const SELECTED: Color = Color(0x0202_02ff);

/// A code whose selected cells take a background from `data-selected:`.
#[component]
fn Selected() -> impl IntoElement {
    view().child(
        view().test_id("otp").child(
            sized(otp_input())
                .length(6u32)
                .cell_class("data-selected:bg-[#020202]")
                .accessibility_label("Code"),
        ),
    )
}

/// [`Selected`], and which views its latest styles mark.
fn start_selected() -> (Headless, Arc<Mutex<HashMap<NodeId, bool>>>) {
    let mut app = Headless::start(EngineConfig {
        viewport: Size::new(px(390.0), px(844.0)),
        ..EngineConfig::new(Root::of::<Selected>())
    })
    .unwrap();
    let styles = Arc::new(Mutex::new(HashMap::new()));
    let seen = styles.clone();
    app.observe(move |_, batch| {
        let mut seen = seen.lock().unwrap();
        for item in &batch.items {
            match item {
                MountItem::ViewStyle { id, style } => {
                    seen.insert(*id, style.background_color == SELECTED);
                }
                MountItem::Delete { id } => {
                    seen.remove(id);
                }
                _ => {}
            }
        }
    });
    app.first_render().unwrap();
    (app, styles)
}

fn marked_cells(styles: &Mutex<HashMap<NodeId, bool>>) -> usize {
    styles.lock().unwrap().values().filter(|f| **f).count()
}

/// The x positions of the marked cells' frames, for [`the_highlight_is_on_the_selected_cell`].
fn marked_positions(app: &Headless, styles: &Mutex<HashMap<NodeId, bool>>) -> Vec<f32> {
    let mut xs: Vec<f32> = styles
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, marked)| **marked)
        .filter_map(|(id, _)| app.screen().bounds_in_root(*id))
        .map(|b| b.origin.x.0)
        .collect();
    xs.sort_by(f32::total_cmp);
    xs
}

/// SCRATCH: cut-equivalent deletion then a burst — reproduction probe.
#[test]
fn scratch_delete_then_burst() {
    for controlled in [true, false] {
        let mut app = if controlled { start::<Late>() } else { start::<Uncontrolled>() };
        app.type_text("id:otp", "123").unwrap();
        app.wait(Duration::from_millis(200)).unwrap();
        app.select("id:otp", 1, 2).unwrap();
        app.press_key("backspace").unwrap();
        app.wait(Duration::from_millis(100)).unwrap();
        app.type_text("id:otp", "56").unwrap();
        app.wait(Duration::from_millis(200)).unwrap();
        assert_eq!(field(&mut app, "id:otp"), "1563", "controlled={controlled}");
    }
}
