//! `OtpInput`: a fixed-length numeric one-time code (an SMS code, an
//! authenticator's), entered into cells, with no look of its own.
//!
//! `gpui-base`'s `OtpState` keeps the code and edits it from key presses
//! (`otp_input.rs`); its styled counterpart draws the cells. Here one native
//! text field, transparent and over the cells, owns the keyboard: the
//! platform's typing, input methods, paste, the numeric keypad, the
//! one-time-code autofill of iOS and Android and the text field screen
//! readers know. The cells are parts that show the code.
//!
//! - **Edits** are read from what the field reports: the characters it lost
//!   at its end are removed from the code (Backspace removes the last
//!   digit), the digits it gained are added at the end while the code is
//!   shorter than `length` (full-width digits as ASCII ones, anything else
//!   dropped), and text bringing `length` digits or more at once (a paste,
//!   an autofilled code) replaces the code with its first `length`. The
//!   caret stays at the end, as in `gpui-base`, so nothing is edited in the
//!   middle.
//! - **Edits build on the code as the user typed it**: a controlled
//!   `value` comes back a JS turn after `onChange`, maybe after the next
//!   key; the changes the input reported coming back are not set again, a
//!   `value` the app sets otherwise is.
//! - **The field holds the code**: what the edit dropped is taken out of it
//!   again, in a task of its own, since the field's text has already moved
//!   on to what the user typed and the binding's last value is the code.
//! - **The cells**: the one that takes the next digit (`min(len, length -
//!   1)`) is `focus:` while the field has the focus, with a blinking caret
//!   while it is empty (`gpui-base`'s `BlinkCursor`: 500 ms, held for 300 ms
//!   after an edit; steady with reduced motion). A cell that holds a digit
//!   has the attribute `data-filled` (`data-filled:` classes, and
//!   `group-data-filled:` inside a `group` cell), masked or not. In a
//!   terminal an empty cell shows its slot, `·` muted
//!   ([`crate::terminal::OTP_EMPTY`]), a masked digit `●`
//!   ([`crate::terminal::OTP_MASK`]), and the terminal's own cursor is the
//!   caret, in the middle Cell of the cell that takes the next digit.

use std::time::Duration;

use nsh_core::{
    AutoFill, TouchEvent, TouchPhase,
    element::{Style, TextPartStyle},
    prelude::*,
};

use crate::input;

/// How long the caret shows and hides while it blinks (`gpui-base`'s
/// `BlinkCursor` `INTERVAL`).
const BLINK: Duration = Duration::from_millis(500);
/// How long the caret holds after an edit before it blinks again
/// (`BlinkCursor`'s `PAUSE_DELAY`).
const BLINK_PAUSE: Duration = Duration::from_millis(300);
/// What a filled cell shows while `masked`: the bullet the platforms' secure
/// fields show.
const MASK: &str = "\u{2022}";

/// The props of [`OtpInput`].
#[derive(Debug, Clone, PartialEq, Props)]
pub struct OtpInputProps {
    /// How many digits the code has: one cell each.
    pub length: Prop<u32>,
    /// The code (controlled): the app owns it and sets it in `onChange`.
    /// Shown as it is, unfiltered (as `gpui-base`'s `set_value`). Without
    /// it the input keeps its own code.
    pub value: Prop<Option<SharedString>>,
    /// The code at first, while `value` is not set.
    pub default_value: Prop<Option<SharedString>>,
    /// Filled cells show a bullet instead of their digit, and the field is
    /// a secure one: screen readers do not read the code, and the system
    /// keeps it out of its suggestions.
    #[prop(default)]
    pub masked: Prop<bool>,
    /// No focus and no editing; `disabled:` styles apply to the input and
    /// its parts, and screen readers say it is dimmed.
    #[prop(default)]
    pub disabled: Prop<bool>,
    /// How many groups the cells are split into, as evenly as they go
    /// (`gpui-component`'s `OtpInput::groups`): 3 + 3 for six digits in two.
    /// At least one, at most one per cell.
    #[prop(default = 1)]
    pub groups: Prop<u32>,
    /// What screen readers say for the field: what the code is for, and how
    /// many digits it has (the cells are hidden from them).
    pub accessibility_label: Prop<Option<SharedString>>,
    /// The Tailwind classes of each cell: its size, border and text.
    /// `focus:` styles the cell that takes the next digit while the input
    /// has the focus, `data-filled:` the cells that hold a digit (masked or
    /// not), `disabled:` follows the input.
    pub cell_class: Prop<Option<SharedString>>,
    /// The style of each cell, over `cellClass`: its text styles too
    /// (`color`, `fontSize`), which its digit inherits.
    #[prop(default)]
    pub cell_style: Prop<TextPartStyle>,
    /// The Tailwind classes of each group of cells (`flex-row gap-2`).
    /// `disabled:` follows the input.
    pub group_class: Prop<Option<SharedString>>,
    /// The style of each group, over `groupClass`.
    #[prop(default)]
    pub group_style: Prop<Style>,
    /// The Tailwind classes of the caret, in the empty cell that takes the
    /// next digit while the input has the focus: its width, height and
    /// color (`h-5 w-0.5 bg-foreground`). It blinks unless the user asked
    /// for less motion.
    pub caret_class: Prop<Option<SharedString>>,
    /// The style of the caret, over `caretClass`.
    #[prop(default)]
    pub caret_style: Prop<Style>,
}

/// What [`OtpInputEvent::Complete`] reports.
#[derive(Debug, Clone, Default, PartialEq, JsObject)]
#[non_exhaustive]
pub struct OtpInputCompleteEvent {
    /// The code, `length` digits.
    pub value: SharedString,
}

/// The events of an [`OtpInput`].
#[derive(Debug, Clone, PartialEq, Event)]
pub enum OtpInputEvent {
    /// The user changed the code (typed, deleted, pasted, autofilled): the
    /// new code. A `value` the app passes is never reported.
    #[args(value)]
    Change(SharedString),
    /// The user's change filled the last cell, after its `onChange`: the
    /// code, to verify.
    #[args(event)]
    Complete(OtpInputCompleteEvent),
    /// The input took the keyboard focus.
    Focus,
    /// The input lost the keyboard focus.
    Blur,
}

/// The methods of an [`OtpInput`]'s ref.
pub struct OtpInputApi {
    field: ElementRef,
}

#[methods]
impl OtpInputApi {
    /// Takes the keyboard focus (showing the software keyboard), as a
    /// press on the input does (`gpui-base`'s `OtpState::focus`).
    pub fn focus(&self) {
        self.field.focus();
    }
}

/// The code, the app's or the input's own.
#[derive(Clone, Copy)]
struct Code {
    controlled: Prop<Option<SharedString>>,
    own: Signal<SharedString>,
}

impl Code {
    fn get(self) -> SharedString {
        self.controlled.get().unwrap_or_else(|| self.own.get())
    }

    fn get_untracked(self) -> SharedString {
        self.controlled
            .get_untracked()
            .unwrap_or_else(|| self.own.get_untracked())
    }
}

/// How many reported codes the input waits to see come back as `value`:
/// a few keys a JS turn behind, not an app that keeps its code.
const ECHOES: usize = 16;

/// Takes `value` out of `echoes`, the codes the input reported and has not
/// seen come back, with those reported before it: whether it is one of
/// them (the app took the user's change, maybe a key or more behind).
pub(crate) fn echoed(echoes: &mut Vec<String>, value: &str) -> bool {
    match echoes.iter().position(|e| e == value) {
        Some(at) => {
            echoes.drain(..=at);
            true
        }
        None => false,
    }
}

/// The caret's blinking (`gpui-base`'s `BlinkCursor`): its opacity, moved
/// natively by a looping animation, so a blink costs the engine nothing
/// (no task wakes, no view comes or goes).
#[derive(Clone, Copy)]
struct Blink {
    opacity: AnimatedValue,
    run: Stored<Option<Animation>>,
}

impl Blink {
    fn new() -> Self {
        Self {
            opacity: animated_value(1.0),
            run: stored(None),
        }
    }

    /// Stops blinking, where it is.
    fn stop(self) {
        if let Some(run) = self.run.replace(None) {
            run.stop();
        }
    }

    /// Shows the caret, then blinks it after `hold` (none with reduced
    /// motion, nor in a terminal, whose own cursor stands for the caret):
    /// hidden for `BLINK`, shown for `BLINK`, and so on.
    fn start(self, hold: Duration) {
        self.stop();
        self.opacity.set(1.0);
        if untracked(reduce_motion) || crate::terminal::is_terminal() {
            return;
        }
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let jump = Easing::Steps(1, StepPosition::JumpEnd);
        let frames = [
            Keyframe::at(0.0, 0.0).ease(jump),
            Keyframe::at(0.5, 1.0).ease(jump),
            Keyframe::at(1.0, 0.0),
        ];
        let blink = Animation::keyframes(
            &self.opacity,
            frames,
            KeyframeTiming::new(2.0 * ms(BLINK))
                .easing(Easing::Linear)
                .infinite(),
        );
        // A delay before the loop, not the keyframes' own: theirs would
        // hold the first keyframe, hidden.
        let run = Animation::sequence([Animation::delay(ms(hold)), blink]);
        run.start();
        self.run.replace(Some(run));
    }
}

/// A one-time-code input: a fixed number of digits entered into cells (a
/// verification code from a text message, an authenticator's code). It owns
/// its behavior (typing, Backspace, paste and the system's code autofill,
/// digits only, the length, the focus, the caret, the masked state, one
/// text field for screen readers) and leaves its look to the app: no
/// colors, sizes, borders or label of its own. The code is the app's
/// (`value` and `onChange`) or its own (`defaultValue`); `onComplete` tells
/// when the user filled it. Its parts are the cells (`cellClass` /
/// `cellStyle`, `focus:` on the one that takes the next digit while
/// focused, `data-filled:` on those that hold a digit), the groups they
/// are split into (`groups`, `groupClass` / `groupStyle`) and the caret
/// (`caretClass` / `caretStyle`); the app's
/// `class` and `style` lay out and style the input (`flex-row gap-4` for
/// its groups in a row). A press anywhere on it focuses it.
#[component(methods = OtpInputApi)]
pub fn OtpInput(props: OtpInputProps, cx: Scope<OtpInputEvent>) -> impl IntoElement {
    let code = Code {
        controlled: props.value,
        own: signal(props.default_value.get_untracked().unwrap_or_default()),
    };
    // What the field holds, as far as the input knows: the code, or what
    // the user just typed until it is taken back to the code.
    let field = signal(code.get_untracked().to_string());
    // The code as the user edited it: what the next edit applies to. A
    // controlled `value` comes back a JS turn later, so keys typed before
    // it does build on this, not on the `value` shown.
    let typed = stored(code.get_untracked().to_string());
    // The codes reported and not yet seen come back as `value`.
    let echoes = stored(Vec::<String>::new());
    let correction = stored(None::<Spawned<()>>);
    let focused = signal(false);
    let blink = Blink::new();
    let field_ref = element_ref();
    // The field's selection as the platform reports it: the cells draw it,
    // since the field's own highlight is transparent (its glyphs do not
    // sit on the cells, so a native highlight would lie).
    let selection = signal(Selection::default());
    cx.methods(OtpInputApi { field: field_ref });

    // A code the app sets (or the own one) is the code edits apply to and
    // the field's text, unless it is a change the user made coming back:
    // newer keys may have built on it already.
    effect(move || {
        let value = code.get().to_string();
        untracked(move || {
            if echoes.with(|echoes| echoed(echoes, &value)) {
                return;
            }
            echoes.with(Vec::clear);
            typed.replace(value.clone());
            if field.get() != value {
                field.set(value);
            }
        });
    });

    let parts = Cells {
        length: props.length,
        masked: props.masked,
        disabled: props.disabled,
        class: props.cell_class,
        style: props.cell_style,
        caret_class: props.caret_class,
        caret_style: props.caret_style,
        code,
        focused,
        blink,
        selection,
        terminal: crate::terminal::is_terminal(),
        frames: signal(Vec::new()),
    };

    let changed = move |text: &str| {
        if let Some(task) = correction.replace(None) {
            task.cancel();
        }
        let before = field.get_untracked();
        let (deleted, inserted) = diff(&before, text);
        let shown = typed.get();
        let next = edit(
            &shown,
            deleted,
            inserted,
            props.length.get_untracked() as usize,
        );
        if next != shown {
            typed.replace(next.clone());
            echoes.with(|echoes| {
                if echoes.len() == ECHOES {
                    echoes.remove(0);
                }
                echoes.push(next.clone());
            });
            if code.controlled.get_untracked().is_none() {
                code.own.set(next.clone().into());
            }
            if focused.get_untracked() {
                blink.start(BLINK_PAUSE);
            }
            cx.emit(OtpInputEvent::Change(next.clone().into()));
            if next.chars().count() == props.length.get_untracked() as usize {
                cx.emit(OtpInputEvent::Complete(OtpInputCompleteEvent {
                    value: next.clone().into(),
                }));
            }
        }
        if next == text {
            field.set(next);
        } else if next != before {
            // The binding's last value differs: setting it sends it.
            field.set(next.clone());
            field_ref.set_selection(utf16_len(&next), utf16_len(&next));
        } else {
            // The binding's last value is the code already (the edit
            // changed nothing): the field's text goes there first, then
            // back, in a run of its own, so the field gets the code.
            field.set(text.to_owned());
            let task = spawn(async move {
                field.set(next.clone());
                field_ref.set_selection(utf16_len(&next), utf16_len(&next));
            });
            correction.replace(Some(task));
        }
    };

    let (groups, group_class, group_style) = (props.groups, props.group_class, props.group_style);
    let per_group = computed(move || {
        let length = parts.length.get() as usize;
        length
            .div_ceil(group_count(length, groups.get() as usize))
            .max(1)
    });
    // A terminal shows its own cursor at the input (`caret(false)`, no
    // width): on the cell that takes the next digit, where it goes.
    let group_frames = signal(Vec::new());
    let sink = computed(move || {
        let ix = parts.active();
        let cell = parts
            .frames
            .with(|f: &Vec<Bounds<Points>>| f.get(ix).copied())?;
        let group =
            group_frames.with(|f: &Vec<Bounds<Points>>| f.get(ix / per_group.get()).copied())?;
        let metrics = text_metrics("", "0");
        let (width, height) = (metrics.width, metrics.height);
        let middle = |size: Points, step: Points| {
            step * (crate::terminal::columns_in(size, step).saturating_sub(1) / 2) as f32
        };
        Some(Point::new(
            group.origin.x + cell.origin.x + middle(cell.size.width, width),
            group.origin.y + cell.origin.y + middle(cell.size.height, height),
        ))
    });
    let cells = dynamic(move || {
        let length = parts.length.get() as usize;
        let per_group = per_group.get();
        let mut rows: Vec<Vec<AnyElement>> = Vec::new();
        for ix in 0..length {
            if ix % per_group == 0 {
                rows.push(Vec::new());
            }
            rows.last_mut().expect("a group").push(parts.cell(ix));
        }
        rows.into_iter()
            .enumerate()
            .map(|(g, cells)| {
                // The field tells screen readers the code.
                view()
                    .on_layout(move |b: &Bounds<Points>| put(group_frames, g, *b))
                    .accessibility_elements_hidden(true)
                    .project_disabled(parts.disabled)
                    .app_class(group_class)
                    .inline_style(group_style)
                    .children(cells)
            })
            .collect::<Vec<_>>()
    });

    // A tap on the cells places the caret where it lands. The field's own
    // placement cannot: its glyphs are one continuous run at its own font
    // metrics, with no relation to the cells' positions, so on the phones
    // every tap right of the short glyph run clamps to the end of the text.
    // The cells' frames are known here, and the field covers the input
    // exactly (inset 0, no padding), so the touch's location in it is in
    // the input's own coordinates. The correction goes out when the touch
    // ends in a bare caret: a long press's word selection, a double tap's,
    // and a handle drag are selections by then and keep what the platform
    // gave them. The write is count-guarded, so a key typed before it
    // lands wins over it. A long press's own selection is still on its way
    // to the report when the touch ends, so the touch that raised one is
    // remembered here, not left to the lagging signal.
    let long_pressed = stored(false);
    let place_caret = move |e: &TouchEvent| {
        if e.phase == TouchPhase::Start {
            long_pressed.replace(false);
        }
        if e.phase != TouchPhase::End || parts.disabled.get_untracked() {
            return;
        }
        if long_pressed.get() {
            return;
        }
        if selection.get_untracked().start != selection.get_untracked().end {
            return;
        }
        let Some(point) = e.points.iter().find(|p| p.changed).or(e.points.first()) else {
            return;
        };
        let x = point.location.x.0;
        let length = parts.length.get_untracked() as usize;
        if length == 0 {
            return;
        }
        let per_group = per_group.get_untracked();
        let Some(position) = parts.frames.with_untracked(|cells| {
            group_frames.with_untracked(|groups| caret_at(x, length, per_group, cells, groups))
        }) else {
            return;
        };
        field_ref.set_selection(position, position);
    };

    // A long press anywhere on the input selects the code whole, as the
    // platform's own word selection does with a run of digits. It cannot
    // come from the field's own hit-test: the hidden glyphs sit in one
    // small strip at the field's start, so on the phones a long press
    // over a cell right of the strip finds no word and selects nothing
    // (spreading the glyphs across the cells' width was the other shape
    // — but the cells are grouped, so no uniform pitch lines them up,
    // and the run's positions would still be the font's, not the cells').
    // The write is the field's real selection: the edit menu and the
    // handles are the platform's own, and it is count-guarded, so a key
    // typed before it lands wins over it.
    let select_run = move || {
        if parts.disabled.get_untracked() {
            return;
        }
        let len = utf16_len(&field.get_untracked());
        if len > 0 {
            long_pressed.replace(true);
            field_ref.set_selection(0, len);
        }
    };

    view()
        .disabled(props.disabled)
        .on_long_press(select_run)
        // In a terminal the field is no wider than a Cell, under none of
        // the cells: a press on them gives it the keys, as the field
        // over them does elsewhere.
        .when(parts.terminal, |v| {
            v.on_touch(move |e: &TouchEvent| {
                if e.phase == TouchPhase::Start && !parts.disabled.get_untracked() {
                    field_ref.focus();
                }
            })
        })
        .when(!parts.terminal, |v| v.on_touch(place_caret))
        .child(cells)
        .child(
            input()
                .element_ref(&field_ref)
                .absolute()
                .when(!parts.terminal, |v| v.inset_0())
                .when(parts.terminal, |v| {
                    let at = move || sink.get().unwrap_or_default();
                    v.left(move || Length::from(at().x))
                        .top(move || Length::from(at().y))
                        .w(px(0.0))
                        .h(crate::terminal::cell_height())
                })
                .p_0()
                .border(px(0.0))
                .bg(AdaptiveColor::TRANSPARENT)
                .text_color(AdaptiveColor::TRANSPARENT)
                .selection_color(transparent_black())
                // The platform's own caret stays, only unseen: disabling it
                // (`caret(false)`) is for an editor that draws and manages
                // everything itself, and on the phones it cost the field its
                // edit menu — a long press showed no Paste, though paste is
                // this input's own design. Transparent is invisible without
                // disabling anything.
                .caret_color(transparent_black())
                // A terminal draws no caret in the field: its own cursor
                // stands at the cell that takes the next digit.
                .when(parts.terminal, |v| v.caret(false))
                .keyboard_type(KeyboardType::Number)
                .auto_fill(AutoFill::OneTimeCode)
                .auto_correct(false)
                .auto_capitalize(AutoCapitalize::None)
                .masked(props.masked)
                .disabled(props.disabled)
                .optional_label(props.accessibility_label)
                .value(move || field.get())
                .on_change(move |text: &str| changed(text))
                .on_selection_change(move |reported: &Selection| {
                    selection.set(*reported);
                    // The caret is the user's own, wherever they put it:
                    // a selection is theirs for the edit menu, and a caret
                    // inside the text is where an arrow or a click left
                    // it. The reconstruction reads any single contiguous
                    // edit exactly — what the field lost, wherever it lost
                    // it, comes out of the code and the rest close up — so
                    // nothing is pinned to the end. Pinning was the burst
                    // fault itself: measured against the binding, which
                    // lags a JS turn behind what the user typed, it yanked
                    // the caret into the text mid-burst and the next key
                    // landed inside it; and it undid the caret moves a
                    // selection starts with, so a cut of the first digits
                    // cut the last. The rewrite after a correction places
                    // the caret at the end itself, count-guarded.
                })
                .on_focus(move || {
                    focused.set(true);
                    blink.start(BLINK);
                    cx.emit(OtpInputEvent::Focus);
                })
                .on_blur(move || {
                    focused.set(false);
                    blink.stop();
                    cx.emit(OtpInputEvent::Blur);
                }),
        )
}

/// What the cells show and read.
#[derive(Clone, Copy)]
struct Cells {
    length: Prop<u32>,
    masked: Prop<bool>,
    disabled: Prop<bool>,
    class: Prop<Option<SharedString>>,
    style: Prop<TextPartStyle>,
    caret_class: Prop<Option<SharedString>>,
    caret_style: Prop<Style>,
    code: Code,
    focused: Signal<bool>,
    blink: Blink,
    selection: Signal<Selection>,
    /// In a terminal: the cells' frames (in their group), for its cursor.
    terminal: bool,
    frames: Signal<Vec<Bounds<Points>>>,
}

/// Keeps `bounds` as the frame of item `ix`.
fn put(frames: Signal<Vec<Bounds<Points>>>, ix: usize, bounds: Bounds<Points>) {
    if frames.with_untracked(|f| f.get(ix) == Some(&bounds)) {
        return;
    }
    frames.update(|f| {
        if f.len() <= ix {
            f.resize(ix + 1, Bounds::default());
        }
        f[ix] = bounds;
    });
}

impl Cells {
    /// The cell that takes the next digit (`gpui-component`'s `cursor_ix`).
    fn active(self) -> usize {
        let length = self.length.get() as usize;
        self.code
            .get()
            .chars()
            .count()
            .min(length.saturating_sub(1))
    }

    /// Whether the field's selection covers cell `ix`: one unit of it per
    /// digit (ASCII), so the cell is inside when `[ix, ix + 1)` meets
    /// `[start, end)`.
    fn selected(self, ix: usize) -> bool {
        let selection = self.selection.get();
        selection.start != selection.end
            && selection.start < ix as u32 + 1
            && selection.end > ix as u32
    }

    /// Cell `ix`: its digit (a bullet while masked) and `data-filled`, or
    /// the caret while it is the empty cell that takes the next digit and
    /// the input has the focus, over the selection highlight when the
    /// field's selection covers it.
    fn cell(self, ix: usize) -> AnyElement {
        let digit = move || self.code.get().chars().nth(ix);
        let frames = self.frames;
        view()
            .on_layout(move |b: &Bounds<Points>| put(frames, ix, *b))
            .project_focused(move || {
                self.focused.get() && !self.disabled.get() && self.active() == ix
            })
            .project_disabled(self.disabled)
            .project_data("filled", move || digit().is_some())
            .project_data("selected", move || self.selected(ix))
            .app_class(self.class)
            .inline_style(move || self.style.get())
            .child(
                // The platform's selection, which the field's own
                // transparent highlight cannot show: it is drawn on the
                // cells it really covers, behind the digit.
                when(
                    move || !self.terminal && self.selected(ix),
                    move || {
                        view()
                            .absolute()
                            .inset_0()
                            .bg(crate::theme::theme().selection())
                    },
                ),
            )
            .child(
                when(
                    move || digit().is_some(),
                    move || {
                        text(move || match digit() {
                            Some(_) if self.masked.get() => match self.terminal {
                                true => crate::terminal::OTP_MASK.to_owned(),
                                false => MASK.to_owned(),
                            },
                            Some(c) => c.to_string(),
                            None => String::new(),
                        })
                    },
                )
                .otherwise(move || {
                    // A terminal's empty cell shows its slot, muted; its
                    // caret is the terminal's own cursor, at the field (the
                    // sink above), over the slot's Cell.
                    if self.terminal {
                        return Some(
                            text(crate::terminal::OTP_EMPTY)
                                .text_color(crate::terminal::tone_color(
                                    crate::terminal::Tone::Muted,
                                ))
                                .into_element(),
                        );
                    }
                    // Mounted while the cell takes the next digit; it
                    // blinks natively.
                    Some(
                        when(
                            move || !self.terminal && self.focused.get() && self.active() == ix,
                            move || {
                                view()
                                    .app_class(self.caret_class)
                                    .inline_style(self.caret_style)
                                    .bind_opacity(&self.blink.opacity)
                            },
                        )
                        .into_element(),
                    )
                }),
            )
            .into_element()
    }
}

/// How many groups `requested` splits `length` cells into (`gpui-component`'s
/// `OtpInput::resolved_groups`): at least one, at most one per cell.
fn group_count(length: usize, requested: usize) -> usize {
    requested.max(1).min(length.max(1))
}

/// The caret position nearest to `x` (in the input's coordinates): position
/// `p` sits at the left edge of cell `p` (the right edge of the last cell
/// for `length`), the cells' frames in their groups and the groups' in the
/// input. `None` before the frames are known.
fn caret_at(
    x: f32,
    length: usize,
    per_group: usize,
    cells: &[Bounds<Points>],
    groups: &[Bounds<Points>],
) -> Option<u32> {
    let per_group = per_group.max(1);
    let edge = |p: usize| -> Option<f32> {
        if p < length {
            let cell = cells.get(p)?;
            let group = groups.get(p / per_group)?;
            Some(group.origin.x.0 + cell.origin.x.0)
        } else {
            let last = length.checked_sub(1)?;
            let cell = cells.get(last)?;
            let group = groups.get(last / per_group)?;
            Some(group.origin.x.0 + cell.origin.x.0 + cell.size.width.0)
        }
    };
    let mut best: Option<(u32, f32)> = None;
    for p in 0..=length {
        let Some(ex) = edge(p) else { continue };
        let distance = (ex - x).abs();
        if best.is_none_or(|(_, d)| distance < d) {
            best = Some((p as u32, distance));
        }
    }
    // Every edge must be known: a half-laid-out input has no honest answer.
    (0..=length).try_fold((), |_, p| edge(p).map(|_| ()))?;
    Some(best?.0)
}

/// `c` as an ASCII digit: itself, or a full-width digit's (`０`–`９`, an
/// East Asian input method's) as `gpui-base`'s `to_digit_char` maps them.
pub(crate) fn digit(c: char) -> Option<char> {
    if c.is_ascii_digit() {
        return Some(c);
    }
    let n = (c as u32).checked_sub('０' as u32)?;
    char::from_digit(n, 10)
}

/// What the field lost at its end (characters) and what it gained, from
/// `before` to `after`.
pub(crate) fn diff<'a>(before: &str, after: &'a str) -> (usize, &'a str) {
    let prefix = before
        .char_indices()
        .zip(after.chars())
        .find(|((_, a), b)| a != b)
        .map_or_else(|| before.len().min(after.len()), |((i, _), _)| i);
    // `prefix` is a boundary of both: the strings agree up to it.
    let deleted = before[prefix..].chars().count();
    (deleted, &after[prefix..])
}

/// The code after an edit of `shown` that removed `deleted` characters at
/// its end and inserted `inserted`: its digits appended while the code is
/// shorter than `length`, or, `length` of them at once, the code replaced
/// by the first `length`.
fn edit(shown: &str, deleted: usize, inserted: &str, length: usize) -> String {
    let digits: Vec<char> = inserted.chars().filter_map(digit).collect();
    if length > 0 && digits.len() >= length {
        return digits[..length].iter().collect();
    }
    let mut chars: Vec<char> = shown.chars().collect();
    chars.truncate(chars.len().saturating_sub(deleted));
    for d in digits {
        if chars.len() >= length {
            break;
        }
        chars.push(d);
    }
    chars.into_iter().collect()
}

/// `text`'s length in UTF-16 units, as inputs count their selection.
pub(crate) fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::{diff, digit, echoed, edit, group_count};

    #[test]
    fn a_reported_code_coming_back_late_is_an_echo() {
        let mut echoes = vec!["1".to_owned(), "13".to_owned(), "135".to_owned()];
        // The app took "1" while "13" and "135" were typed.
        assert!(echoed(&mut echoes, "1"));
        assert_eq!(echoes, ["13", "135"]);
        assert!(echoed(&mut echoes, "135"));
        assert!(echoes.is_empty());
        // A code the app sets itself is none.
        assert!(!echoed(&mut echoes, ""));
    }

    #[test]
    fn keys_append_digits_up_to_the_length_and_backspace_removes_the_last() {
        // `gpui-base`'s `keyboard_editing_backspaces_and_stops_at_length`.
        assert_eq!(edit("12", 1, "", 4), "1");
        assert_eq!(edit("12", 0, "３", 4), "123");
        assert_eq!(edit("1234", 0, "5", 4), "1234");
        assert_eq!(edit("12", 0, "a", 4), "12");
        assert_eq!(edit("", 1, "", 4), "");
    }

    #[test]
    fn a_whole_code_at_once_replaces_the_code() {
        assert_eq!(edit("12", 0, "987-654", 6), "987654");
        assert_eq!(edit("123456", 0, "9876543", 6), "987654");
        // Fewer digits than the length are appended.
        assert_eq!(edit("12", 0, "3-4", 6), "1234");
        assert_eq!(edit("1234", 0, "5678", 6), "123456");
    }

    #[test]
    fn full_width_digits_are_ascii_and_nothing_else_is_a_digit() {
        assert_eq!(digit('０'), Some('0'));
        assert_eq!(digit('９'), Some('9'));
        assert_eq!(digit('7'), Some('7'));
        assert_eq!(digit('a'), None);
        assert_eq!(digit('٣'), None);
        assert_eq!(digit('-'), None);
    }

    #[test]
    fn the_field_reports_what_it_lost_and_gained_at_its_end() {
        assert_eq!(diff("12", "123"), (0, "3"));
        assert_eq!(diff("123", "12"), (1, ""));
        assert_eq!(diff("12", "12"), (0, ""));
        assert_eq!(diff("123", ""), (3, ""));
        assert_eq!(diff("1２", "1２3"), (0, "3"));
        assert_eq!(diff("1２", "13"), (1, "3"));
        // A secure field typed in again starts over.
        assert_eq!(diff("1234", "5"), (4, "5"));
    }

    #[test]
    fn invalid_group_counts_are_safely_clamped() {
        // `gpui-component`'s test of `resolved_groups`.
        assert_eq!(group_count(6, 0), 1);
        assert_eq!(group_count(6, 20), 6);
        assert_eq!(group_count(0, 0), 1);
        assert_eq!(group_count(5, 2), 2);
    }
}
