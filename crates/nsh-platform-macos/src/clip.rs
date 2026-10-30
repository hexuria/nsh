//! Whether the views put off out of sight ([`crate::mount::Mounter`]'s
//! stale views) show now, or are near showing, at a cost per view that does
//! not grow with its depth.
//!
//! [`NshView::shown`] walks every ancestor of a view: hundreds of list rows'
//! texts put off out of sight cost that walk on every frame of a scroll,
//! as a scroll may bring any of them in. Yet a scroll moves only the
//! scroll view's offset: what lies between a view and its nearest scroll
//! view (its *top*) stays where it was. So a view's rectangle in its top's
//! content space, cut by the clipping ancestors in between, is kept
//! ([`Clips`]) until views move, are inserted or removed, or hide
//! ([`moved`]); each pass works out once per top what it and its own
//! ancestors leave visible ([`Pass`]), and a view shows when the two
//! meet.

use std::cell::{Cell, RefCell};

use nsh_core::{NodeId, ROOT_ID};
use objc2_foundation::NSRect;
use rustc_hash::FxHashMap;

use crate::view::NshView;

/// A rectangle as `(x0, y0, x1, y1)`.
pub(crate) type Span = (f32, f32, f32, f32);

/// An ancestor as the walks take it: its placed frame (in its parent's
/// content space), the origin its content shows from (a scroll view's
/// offset, (0, 0) otherwise) and whether it clips its children.
type Step = (Span, (f32, f32), bool);

/// All of a content space: nothing cut yet.
const ALL: Span = (
    f32::NEG_INFINITY,
    f32::NEG_INFINITY,
    f32::INFINITY,
    f32::INFINITY,
);

thread_local! {
    /// Bumped whenever views move relative to their tops ([`moved`]).
    static EPOCH: Cell<u64> = const { Cell::new(0) };
}

/// Views moved, were inserted or removed, or hid: the rectangles kept in
/// tops' spaces are stale. Scroll offsets are not such a move (each pass
/// reads them).
pub(crate) fn moved() {
    EPOCH.with(|e| e.set(e.get().wrapping_add(1)));
}

fn epoch() -> u64 {
    EPOCH.with(Cell::get)
}

/// Where a view sits in its top's content space.
#[derive(Clone)]
struct Inner {
    /// Its nearest scroll view, else the root, else (out of the tree) its
    /// outermost ancestor.
    top: NshView,
    /// Its placed frame moved into the top's content space and cut by
    /// the clipping ancestors in between; `None` when they hide it.
    shown: Option<Span>,
    /// The same with every clip grown by the top's size ([`Clips::near`]);
    /// `None` when they hide it even so, or the top does not scroll.
    near: Option<Span>,
}

/// What a top and its ancestors leave of the top's content space.
#[derive(Clone, Copy)]
struct Outer {
    shown: Option<Span>,
    near: Option<Span>,
}

/// The views' rectangles in their tops' spaces, kept until views move.
#[derive(Default)]
pub(crate) struct Clips {
    epoch: Cell<u64>,
    /// By node id; `None`: the view or one of its ancestors below its top
    /// is hidden.
    inner: RefCell<FxHashMap<NodeId, Option<Inner>>>,
}

/// One pass over views at a moment (no view moves during it): each top's
/// visible part, worked out once.
#[derive(Default)]
pub(crate) struct Pass {
    outer: FxHashMap<NodeId, Outer>,
}

impl Clips {
    /// Forgets every view (the surface is cleared).
    pub(crate) fn clear(&self) {
        self.inner.borrow_mut().clear();
    }

    /// Whether any of `view` can be seen, as [`NshView::shown`] has it.
    pub(crate) fn shown(&self, view: &NshView, pass: &mut Pass) -> bool {
        self.with(view, pass, |inner, outer| meet(inner.shown, outer.shown))
    }

    /// Whether `view` is within a viewport of showing: no more than its
    /// innermost scroll view's width or height past any clipping
    /// ancestor's edge (a list row a scroll brings in next). False when
    /// no ancestor scrolls (nothing brings it in).
    pub(crate) fn near(&self, view: &NshView, pass: &mut Pass) -> bool {
        self.with(view, pass, |inner, outer| meet(inner.near, outer.near))
    }

    fn with(
        &self,
        view: &NshView,
        pass: &mut Pass,
        f: impl FnOnce(&Inner, &Outer) -> bool,
    ) -> bool {
        let now = epoch();
        let mut inner = self.inner.borrow_mut();
        if self.epoch.replace(now) != now {
            inner.clear();
        }
        let Some(inner) = inner.entry(view.id()).or_insert_with(|| inner_of(view)) else {
            return false;
        };
        let outer = pass
            .outer
            .entry(inner.top.id())
            .or_insert_with(|| outer_of(&inner.top));
        f(inner, outer)
    }
}

/// Whether `rect` and what a top leaves visible meet.
fn meet(rect: Option<Span>, visible: Option<Span>) -> bool {
    let (Some(r), Some(v)) = (rect, visible) else {
        return false;
    };
    r.0.max(v.0) < r.2.min(v.2) && r.1.max(v.1) < r.3.min(v.3)
}

/// `view`'s rectangle in its top's space: one walk up to the top.
fn inner_of(view: &NshView) -> Option<Inner> {
    if view.ns().isHidden() {
        return None;
    }
    let mut steps = Vec::new();
    let mut top = view.clone();
    while let Some(p) = top.parent_view() {
        top = p;
        if is_top(&top) {
            break;
        }
        if top.ns().isHidden() {
            return None;
        }
        steps.push(step(&top));
    }
    let rect = placed(view.ns().frame());
    let near = margin(&top).and_then(|m| cut(rect, steps.iter().map(|s| grow(*s, m))));
    Some(Inner {
        shown: cut(rect, steps.iter().copied()),
        near,
        top,
    })
}

/// What `top` and its ancestors leave of its content space: nothing when
/// it is not in a window or hidden.
fn outer_of(top: &NshView) -> Outer {
    let ns = top.ns();
    if ns.window().is_none() || ns.isHiddenOrHasHiddenAncestor() {
        return Outer {
            shown: None,
            near: None,
        };
    }
    let steps: Vec<Step> = std::iter::successors(Some(top.clone()), NshView::parent_view)
        .map(|p| step(&p))
        .collect();
    Outer {
        shown: window(steps.iter().copied()),
        near: margin(top).and_then(|m| window(steps.iter().map(|s| grow(*s, m)))),
    }
}

/// Whether the walk up from a view stops at `view`: a scroll view, whose
/// offset moves without anything below it moving, or the root.
fn is_top(view: &NshView) -> bool {
    let s = view.state();
    s.scroll.is_some() || s.id == ROOT_ID
}

/// How far [`Clips::near`] reaches past a clip: a scrolling top's size.
fn margin(top: &NshView) -> Option<(f32, f32)> {
    top.state().scroll.is_some().then(|| {
        let size = top.ns().frame().size;
        (size.width as f32, size.height as f32)
    })
}

fn step(view: &NshView) -> Step {
    let clips = view.state().clips() || view.state().id == ROOT_ID;
    (
        placed(view.ns().frame()),
        crate::scroll::bounds_origin(view),
        clips,
    )
}

fn placed(f: NSRect) -> Span {
    let (x, y) = (f.origin.x as f32, f.origin.y as f32);
    (x, y, x + f.size.width as f32, y + f.size.height as f32)
}

/// A step with its clip grown by `margin` on every side: the frame and
/// the origin its content shows from move out together, which leaves the
/// step into its parent's space as it was.
fn grow((f, (ox, oy), clips): Step, (mx, my): (f32, f32)) -> Step {
    if !clips {
        return (f, (ox, oy), false);
    }
    (
        (f.0 - mx, f.1 - my, f.2 + mx, f.3 + my),
        (ox - mx, oy - my),
        true,
    )
}

/// `rect` (in the first step's content space) moved through `steps`,
/// innermost first, into the content space of the last one's parent and
/// cut by the clipping ones; `None` once nothing is left.
fn cut(rect: Span, steps: impl IntoIterator<Item = Step>) -> Option<Span> {
    let (mut x0, mut y0, mut x1, mut y1) = rect;
    for (frame, (ox, oy), clips) in steps {
        if clips {
            let (w, h) = (frame.2 - frame.0, frame.3 - frame.1);
            x0 = x0.max(ox);
            y0 = y0.max(oy);
            x1 = x1.min(ox + w);
            y1 = y1.min(oy + h);
            if x1 <= x0 || y1 <= y0 {
                return None;
            }
        }
        // Into the grandparent's content space: the offset origin shows
        // at the frame's origin.
        x0 += frame.0 - ox;
        x1 += frame.0 - ox;
        y0 += frame.1 - oy;
        y1 += frame.1 - oy;
    }
    Some((x0, y0, x1, y1))
}

/// The part of the first step's content space that shows through all of
/// `steps` (innermost first), in that space; `None` when no part does.
/// What [`cut`] leaves of a rectangle is that rectangle met with this,
/// moved: a clip cuts in its own space, which is the first's moved by the
/// steps below it.
fn window(steps: impl IntoIterator<Item = Step>) -> Option<Span> {
    let (mut b, mut dx, mut dy) = (ALL, 0.0, 0.0);
    for (frame, (ox, oy), clips) in steps {
        if clips {
            let (w, h) = (frame.2 - frame.0, frame.3 - frame.1);
            b = (
                b.0.max(ox - dx),
                b.1.max(oy - dy),
                b.2.min(ox + w - dx),
                b.3.min(oy + h - dy),
            );
            if b.2 <= b.0 || b.3 <= b.1 {
                return None;
            }
        }
        dx += frame.0 - ox;
        dy += frame.1 - oy;
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `rect` shows through `steps` as the full walk has it: the
    /// view's own `cut`, the rest as a window.
    fn split(rect: Span, steps: &[Step], at: usize) -> bool {
        let inner = cut(rect, steps[..at].iter().copied());
        meet(inner, window(steps[at..].iter().copied()))
    }

    /// What `revisit` costs a frame of a scroll for `rows` list rows out of
    /// sight, each with a text `depth` views down, put off: the walk of
    /// every ancestor before, a kept rectangle met with the list's window
    /// after (the window check, the same in both, left out: these views
    /// have none). `cargo test --release -p nsh-platform-macos
    /// bench_revisit -- --ignored --nocapture`
    #[test]
    #[ignore = "a benchmark"]
    fn bench_revisit() {
        use nsh_core::{
            ViewKind,
            prelude::{point, px, size},
        };
        // SAFETY: the views never reach a window or the screen; AppKit
        // makes and nests plain views on any thread.
        let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
        let mut id = 1;
        let mut make = |kind, parent: Option<&NshView>, y: f32, h: f32, clips: bool| {
            id += 2;
            let v = NshView::new(mtm, if parent.is_none() { ROOT_ID } else { id }, kind);
            {
                let mut s = v.state_mut();
                s.frame = nsh_core::Bounds {
                    origin: point(px(0.0), px(y)),
                    size: size(px(400.0), px(h)),
                };
                s.style.overflow_hidden = clips;
            }
            if let Some(p) = parent {
                v.insert_before(p, None);
            } else {
                v.place();
            }
            v
        };
        let root = make(ViewKind::View, None, 0.0, 800.0, false);
        let pane = make(ViewKind::View, Some(&root), 0.0, 800.0, false);
        let list = make(ViewKind::Scroll, Some(&pane), 0.0, 800.0, false);
        list.state_mut().scroll = Some(Default::default());
        let (rows, depth) = (400, 8);
        let mut texts = Vec::new();
        for r in 0..rows {
            let mut at = make(ViewKind::View, Some(&list), r as f32 * 300.0, 300.0, false);
            for d in 0..depth {
                at = make(ViewKind::View, Some(&at), 4.0, 280.0, d == 2);
            }
            texts.push(make(ViewKind::Text, Some(&at), 0.0, 20.0, false));
        }
        let frames = 200;
        let time = |f: &mut dyn FnMut(f32) -> usize| {
            let start = std::time::Instant::now();
            let mut n = 0;
            for frame in 0..frames {
                list.state_mut().scroll.as_mut().unwrap().y = frame as f32 * 10.0;
                n += f(frame as f32);
            }
            (start.elapsed() / frames, n)
        };
        let (before, shown_before) = time(&mut |_| {
            texts
                .iter()
                .filter(|t| {
                    !t.ns().isHiddenOrHasHiddenAncestor()
                        && cut(
                            placed(t.ns().frame()),
                            std::iter::successors(t.parent_view(), NshView::parent_view)
                                .map(|p| step(&p)),
                        )
                        .is_some()
                })
                .count()
        });
        let mut kept: FxHashMap<NodeId, Option<Inner>> = FxHashMap::default();
        let (after, shown_after) = time(&mut |_| {
            let mut windows: FxHashMap<NodeId, Option<Span>> = FxHashMap::default();
            texts
                .iter()
                .filter(|t| {
                    let Some(inner) = kept.entry(t.id()).or_insert_with(|| inner_of(t)) else {
                        return false;
                    };
                    let w = *windows.entry(inner.top.id()).or_insert_with(|| {
                        window(
                            std::iter::successors(Some(inner.top.clone()), NshView::parent_view)
                                .map(|p| step(&p)),
                        )
                    });
                    meet(inner.shown, w)
                })
                .count()
        });
        assert_eq!(shown_before, shown_after);
        println!(
            "revisit, {rows} texts {} views deep, per frame: walk {before:?}, kept {after:?} ({shown_before} shown over {frames} frames)",
            depth + 4
        );
    }

    #[test]
    fn a_row_within_a_viewport_of_the_list_is_near() {
        let screen = ((0.0, 0.0, 400.0, 800.0), (0.0, 0.0), true);
        let list = ((0.0, 0.0, 400.0, 600.0), (0.0, 0.0), true);
        let near = |y: f32| {
            let row = (0.0, y, 400.0, y + 40.0);
            cut(
                row,
                [grow(list, (400.0, 600.0)), grow(screen, (400.0, 600.0))],
            )
            .is_some()
        };
        // Below the list's edge by less than its height: near.
        assert!(near(700.0));
        assert!(near(1150.0));
        // A viewport and more away: not yet.
        assert!(!near(1250.0));
        // A view outside no clip is placed as before.
        let plain = ((10.0, 20.0, 50.0, 60.0), (0.0, 0.0), false);
        assert_eq!(grow(plain, (5.0, 5.0)), plain);
        // A grown clip moves its content into the parent's space as it was:
        // the next clip cuts it where it would have.
        let row = (0.0, 100.0, 400.0, 140.0);
        let below = ((0.0, 0.0, 400.0, 90.0), (0.0, 0.0), true);
        assert_eq!(cut(row, [grow(list, (400.0, 600.0)), below]), None);
    }

    #[test]
    fn a_kept_rectangle_meets_the_window_as_the_full_walk_cuts() {
        let screen = ((0.0, 0.0, 400.0, 800.0), (0.0, 0.0), true);
        let list = |y| ((0.0, 40.0, 400.0, 640.0), (0.0, y), true);
        let row = |y| ((16.0, y, 384.0, y + 300.0), (0.0, 0.0), false);
        let bubble = ((0.0, 10.0, 200.0, 290.0), (0.0, 0.0), true);
        let text = (12.0, 8.0, 180.0, 30.0);
        for scrolled in [0.0, 150.0, 600.0, 1000.0, 1180.0, 1300.0, 2000.0] {
            for at_y in [0.0, 500.0, 1200.0] {
                let steps = [bubble, row(at_y), list(scrolled), screen];
                let full = cut(text, steps).is_some();
                // The top is the list: the bubble and the row are kept.
                assert_eq!(
                    split(text, &steps, 2),
                    full,
                    "scrolled {scrolled}, row at {at_y}"
                );
                for at in [0, 1, 3, 4] {
                    assert_eq!(split(text, &steps, at), full, "split at {at}");
                }
            }
        }
    }

    #[test]
    fn a_rectangle_a_clip_below_the_top_hides_stays_hidden() {
        let screen = ((0.0, 0.0, 400.0, 800.0), (0.0, 0.0), true);
        let list = ((0.0, 0.0, 400.0, 600.0), (0.0, 0.0), true);
        // A collapsed panel (overflow hidden, no height) inside the row.
        let panel = ((0.0, 0.0, 400.0, 0.0), (0.0, 0.0), true);
        let text = (0.0, 0.0, 100.0, 20.0);
        assert_eq!(cut(text, [panel]), None);
        assert!(!split(text, &[panel, list, screen], 1));
    }

    #[test]
    fn near_grows_every_clip_as_the_walk_does() {
        let m = (400.0, 600.0);
        let screen = ((0.0, 0.0, 400.0, 800.0), (0.0, 0.0), true);
        let list = ((0.0, 0.0, 400.0, 600.0), (0.0, 0.0), true);
        let cell = ((0.0, 0.0, 400.0, 2000.0), (0.0, 0.0), true);
        for y in [700.0, 1150.0, 1250.0, 1900.0] {
            let row = (0.0, y, 400.0, y + 40.0);
            let steps = [cell, list, screen].map(|s| grow(s, m));
            let full = cut(row, steps).is_some();
            assert_eq!(
                meet(
                    cut(row, steps[..1].iter().copied()),
                    window(steps[1..].iter().copied())
                ),
                full,
                "row at {y}"
            );
        }
    }

    #[test]
    fn no_clip_leaves_all_of_the_space() {
        let moved = ((10.0, 20.0, 50.0, 60.0), (0.0, 0.0), false);
        assert_eq!(window([moved]), Some(ALL));
        // A clip after a move is the clip in the first space, moved back.
        let clip = ((0.0, 0.0, 100.0, 100.0), (0.0, 0.0), true);
        assert_eq!(window([moved, clip]), Some((-10.0, -20.0, 90.0, 80.0)));
        assert_eq!(
            window([clip, ((0.0, 0.0, 5.0, 5.0), (200.0, 0.0), true)]),
            None
        );
    }
}
