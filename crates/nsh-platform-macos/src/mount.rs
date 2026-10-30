//! Applies mount items to the views, in order. Each kind of content has
//! its module ([`crate::text`], [`crate::image`], [`crate::scroll`],
//! [`crate::text_input`], [`crate::navigation`], ...); this file only
//! keeps the view table and the tree.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use nsh_core::{Command, LayoutAnimation, MountItem, NodeId, ROOT_ID, ViewKind, canvas::Damage};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{clip::Clips, surface::Surface, view::NshView};

/// The views of a surface.
pub(crate) struct Mounter {
    /// Every live view by node id (the root included).
    pub(crate) views: FxHashMap<NodeId, NshView>,
    /// The root view.
    pub(crate) root: NshView,
    /// Animates the frame changes, insertions and removals of the rest of
    /// the batch.
    pub(crate) layout_animation: Option<LayoutAnimation>,
    /// The surface lays out right to left.
    pub(crate) rtl: bool,
    /// Views whose drawing changed while they could not be seen
    /// ([`Mounter::draw_when_shown`]).
    stale: RefCell<Vec<NshView>>,
    /// Some of `stale` may be near enough to draw ahead of showing
    /// ([`Mounter::draw_ahead`]): set when one is put off and when views
    /// move, cleared once none is.
    ahead: Cell<bool>,
    /// The views the batch being applied deletes
    /// ([`Mounter::begin_batch`]).
    deleting: FxHashSet<NodeId>,
    /// Where `stale`'s views sit in their scroll views: what
    /// [`Mounter::revisit`] and [`Mounter::draw_ahead`] test each frame.
    clips: Clips,
}

impl Mounter {
    pub(crate) fn new(root: NshView) -> Self {
        let mut views = FxHashMap::default();
        views.insert(ROOT_ID, root.clone());
        Mounter {
            views,
            root,
            layout_animation: None,
            rtl: false,
            stale: RefCell::default(),
            ahead: Cell::new(false),
            deleting: FxHashSet::default(),
            clips: Clips::default(),
        }
    }

    /// Removes every view but the root.
    pub(crate) fn clear(&mut self) {
        for child in self.root.child_views() {
            child.unparent();
        }
        let root = self.root.clone();
        for (_, view) in self.views.drain() {
            if view != root && view.has_parent() {
                view.unparent();
            }
        }
        self.views.insert(ROOT_ID, root);
        self.layout_animation = None;
        self.stale.borrow_mut().clear();
        self.ahead.set(false);
        self.clips.clear();
    }

    /// `view`'s drawing changed in `damage` (on every frame of an animation
    /// of a canvas's values): that is drawn again now if it can be seen,
    /// else the whole view when it shows ([`Mounter::revisit`]). Lists keep
    /// rows out of sight, and a pulse on each of them would otherwise
    /// redraw them all on every frame.
    pub(crate) fn draw_when_shown(&self, view: &NshView, damage: Damage) {
        if view.shown() {
            match damage {
                Damage::Rect(r) => view.redraw_rect(r),
                _ => view.redraw(),
            }
        } else if view.set_stale(true) {
            self.stale.borrow_mut().push(view.clone());
            self.ahead.set(true);
        }
    }

    /// Views moved (a scroll, a batch): views put off may have come near.
    pub(crate) fn moved(&self) {
        if !self.stale.try_borrow().is_ok_and(|s| s.is_empty()) {
            self.ahead.set(true);
        }
    }

    /// Whether views put off may be near enough to draw ahead: the display
    /// link runs for [`Mounter::draw_ahead`].
    pub(crate) fn wants_ahead(&self) -> bool {
        self.ahead.get()
    }

    /// Draws the static content put off out of sight that is within a
    /// viewport of showing ([`Clips::near`], [`NshView::draws_static`]),
    /// until `budget` is spent. Drawn on first show instead, a list row's
    /// texts and canvases all landed in the frame a scroll brought it in
    /// (tens of milliseconds for a long reply or a diagram); drawn here,
    /// they are spread over the frames before it, when the rows mounted
    /// out of sight are still waiting. What is left waits for the next
    /// frame.
    pub(crate) fn draw_ahead(&self, budget: Duration) {
        if !self.ahead.get() {
            return;
        }
        let start = Instant::now();
        let Ok(stale) = self.stale.try_borrow() else {
            return;
        };
        let candidates: Vec<NshView> = stale
            .iter()
            .filter(|v| self.alive(v) && v.draws_static())
            .cloned()
            .collect();
        drop(stale);
        let mut drawn = Vec::new();
        let mut done = true;
        let mut pass = crate::clip::Pass::default();
        for view in candidates {
            if start.elapsed() >= budget {
                done = false;
                break;
            }
            if self.clips.near(&view, &mut pass) {
                view.set_stale(false);
                view.draw_now();
                drawn.push(view);
            }
        }
        if !drawn.is_empty() {
            self.stale.borrow_mut().retain(|v| !drawn.contains(v));
        }
        if done {
            self.ahead.set(false);
        }
    }

    /// Draws the views [`Mounter::draw_when_shown`] put off that can be
    /// seen now (after scrolls, batches and frames). Views out of the tree
    /// wait (their content is drawn once they show); deleted ones are
    /// dropped. Runs on every frame of a scroll: each view costs a lookup
    /// of where it sits in its scroll view, not a walk of its ancestors
    /// ([`crate::clip`]).
    pub(crate) fn revisit(&self) {
        let Ok(mut stale) = self.stale.try_borrow_mut() else {
            return;
        };
        if stale.is_empty() {
            return;
        }
        let mut pass = crate::clip::Pass::default();
        let shown: Vec<NshView> = stale
            .extract_if(.., |v| !self.alive(v) || self.clips.shown(v, &mut pass))
            .collect();
        drop(stale);
        for view in shown {
            view.set_stale(false);
            if self.alive(&view) {
                view.redraw();
                crate::shader::shown(&view);
            }
        }
    }

    /// Whether `view` is still mounted (not deleted).
    fn alive(&self, view: &NshView) -> bool {
        self.views.get(&view.state().id) == Some(view)
    }

    /// A batch is about to be applied: notes what it deletes, so a
    /// subtree leaves its window with its root alone.
    pub(crate) fn begin_batch(&mut self, items: &[MountItem]) {
        self.deleting.clear();
        self.deleting
            .extend(items.iter().filter_map(|item| match item {
                MountItem::Delete { id } => Some(*id),
                _ => None,
            }));
    }

    /// Whether a waiting tree shows, and whether it is near showing: its
    /// root if it went in, else its frame where it goes.
    pub(crate) fn tree_shows(&self, tree: &crate::pace::Tree) -> (bool, bool) {
        if let Some(root) = self.views.get(&tree.root).filter(|r| r.has_parent()) {
            let mut pass = crate::clip::Pass::default();
            return (
                self.clips.shown(root, &mut pass),
                self.clips.near(root, &mut pass),
            );
        }
        let Some(parent) = self.views.get(&tree.parent) else {
            return (false, false);
        };
        crate::clip::child_shows(parent, tree.frame)
    }

    /// The batch is applied.
    pub(crate) fn end_batch(&mut self, surface: &Rc<Surface>) {
        self.deleting.clear();
        self.moved();
        self.layout_animation = None;
        crate::navigation::end_batch(surface);
        crate::system::end_batch(surface);
        crate::input::content_moved(surface, false);
        crate::selectable_view::end_batch(surface);
    }

    fn get(&self, id: NodeId) -> Option<&NshView> {
        let view = self.views.get(&id);
        if view.is_none() {
            log::debug!("mount: no view {id}");
        }
        view
    }

    pub(crate) fn apply(&mut self, surface: &Rc<Surface>, item: &MountItem) {
        match item {
            MountItem::Create { id, kind } => {
                let view = NshView::new(surface.mtm, *id, *kind);
                crate::access::created(&view);
                match kind {
                    ViewKind::Input => crate::text_input::create(surface, &view),
                    ViewKind::Canvas => crate::canvas::create(&view),
                    ViewKind::Image => crate::image::create(&view),
                    ViewKind::WebView | ViewKind::Video | ViewKind::Native => {
                        crate::native::create(surface, &view)
                    }
                    _ => {}
                }
                self.views.insert(*id, view);
            }
            MountItem::TrackStates { id, states } => {
                if let Some(view) = self.get(*id) {
                    crate::input::track_states(surface, view, *states);
                }
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                let (Some(parent), Some(child)) = (self.get(*parent), self.get(*child)) else {
                    return;
                };
                match child.kind() {
                    ViewKind::Modal => return crate::navigation::present(surface, child),
                    ViewKind::HeaderItem => {
                        return crate::navigation::header_item(surface, parent, child);
                    }
                    ViewKind::InputAccessory => {
                        return crate::text_input::accessory(surface, parent, child);
                    }
                    _ => {}
                }
                let before = before
                    .and_then(|b| self.views.get(&b))
                    .filter(|b| b.parent_view().as_ref() == Some(parent));
                if child.has_parent() {
                    child.unparent();
                }
                child.insert_before(parent, before);
                crate::transition::inserted(surface, child, self.layout_animation.as_ref());
                if parent.kind() == ViewKind::Stack {
                    crate::navigation::stack_changed(surface, parent, Some(child));
                }
                if parent.kind() == ViewKind::Scroll {
                    crate::scroll::child_inserted(parent);
                }
            }
            MountItem::Remove { id } => {
                if let Some(view) = self.get(*id) {
                    // A removal animation detaches the view when it ends.
                    if !crate::transition::removing(surface, view, self.layout_animation.as_ref()) {
                        self.detach(surface, view);
                    }
                }
            }
            MountItem::Delete { id } => {
                crate::file_drop::deleted(surface, *id);
                if let Some(view) = self.views.remove(id) {
                    crate::native::deleted(surface, &view);
                    // A view whose parent the batch deletes too stays in it:
                    // the subtree leaves the window with its root, in one
                    // `removeFromSuperview`, not one per view while still
                    // in the window.
                    let inside = view
                        .parent_view()
                        .is_some_and(|p| self.deleting.contains(&p.id()));
                    if inside {
                        // Its running layout animation ends with it.
                        crate::transition::deleted(&view);
                    } else if !crate::transition::removing(
                        surface,
                        &view,
                        self.layout_animation.as_ref(),
                    ) {
                        // No removal animation, which detaches the view
                        // when it ends.
                        crate::transition::deleted(&view);
                        self.detach(surface, &view);
                    }
                }
            }
            MountItem::Frame { id, frame } => {
                let Some(view) = self.get(*id) else { return };
                if *id == ROOT_ID {
                    return;
                }
                let mut frame = *frame;
                if view.kind() == ViewKind::Modal {
                    // Where a modal sits is the platform's (it covers the
                    // window): the core only sizes it.
                    let old = view.state().frame;
                    (frame.origin.x.0, frame.origin.y.0) = (old.origin.x.0, old.origin.y.0);
                }
                let frame = &frame;
                // Compared with where a running layout animation is headed,
                // not with the in-between frame it shows: a frame set back
                // to the shown one must still stop the animation.
                let target = crate::transition::frame_target(view);
                if target.unwrap_or(view.state().frame) == *frame {
                    return;
                }
                let old = std::mem::replace(&mut view.state_mut().frame, *frame);
                view.moved();
                crate::transition::frame_changed(
                    surface,
                    view,
                    old,
                    self.layout_animation.as_ref(),
                );
                match view.kind() {
                    ViewKind::Stack => crate::navigation::stack_resized(surface, view),
                    ViewKind::Canvas => view.changed(),
                    _ => crate::shader::resized(surface, view),
                }
            }
            MountItem::ViewStyle { id, style } => {
                let Some(view) = self.get(*id) else { return };
                let old = std::mem::replace(&mut view.state_mut().style, style.clone());
                if old.overflow_hidden != style.overflow_hidden {
                    crate::clip::moved();
                }
                crate::input::style_changed(view);
                crate::transition::style_changed(surface, view, &old);
                view.moved();
                crate::decor::style_changed(view, &old);
                crate::access::style_changed(view, &old);
            }
            MountItem::Text {
                id,
                text,
                layout,
                content,
            } => {
                if let Some(view) = self.get(*id) {
                    let old = view.state().text.as_ref().map(|t| t.text.clone());
                    crate::text::set(view, text, layout, *content);
                    if let Some(old) = old {
                        crate::transition::text_changed(surface, view, &old, text);
                    }
                    self.draw_when_shown(view, Damage::All);
                    crate::access::text_changed(view);
                }
            }
            MountItem::Events { id, events } => {
                if let Some(view) = self.get(*id) {
                    view.state_mut().events = *events;
                    crate::file_drop::events_changed(surface, view);
                    crate::input::events_changed(surface, view);
                    crate::access::events_changed(view);
                }
            }
            MountItem::Image {
                id,
                source,
                resize_mode,
                options,
            } => {
                if let Some(view) = self.get(*id) {
                    crate::image::set(surface, view, source.as_ref(), *resize_mode, options);
                }
            }
            MountItem::ScrollConfig {
                id,
                horizontal,
                scrollbar_visible,
                replaced,
                mirrored,
                ..
            } => {
                if let Some(view) = self.get(*id) {
                    // A view that scrolls is where the walks up stop.
                    crate::clip::moved();
                    crate::scroll::config(
                        surface,
                        view,
                        *horizontal,
                        *scrollbar_visible,
                        *replaced,
                        *mirrored,
                    );
                }
            }
            MountItem::ScrollContent { id, size } => {
                if let Some(view) = self.get(*id) {
                    crate::scroll::content(surface, view, *size);
                }
            }
            MountItem::ScrollBy { id, dx, dy } => {
                if let Some(view) = self.get(*id) {
                    crate::scroll::scroll_by(surface, view, dx.0, dy.0);
                }
            }
            MountItem::LayoutDirection { rtl } => self.rtl = *rtl,
            MountItem::Overflow { id, extent } => {
                if let Some(view) = self.get(*id) {
                    view.state_mut().overflow = *extent;
                }
            }
            MountItem::Appearance { appearance } => {
                crate::system::set_appearance(surface, *appearance);
            }
            MountItem::Keymap(keymap) => crate::keymap::set(surface, keymap.clone()),
            MountItem::AccessibilityLabel { id, label } => {
                if let Some(view) = self.get(*id) {
                    crate::access::label(view, label);
                }
            }
            MountItem::StickyCells { id, cells } => {
                if let Some(view) = self.get(*id) {
                    crate::scroll::sticky(surface, view, cells.clone());
                }
            }
            MountItem::ScrollBehavior { id, behavior } => {
                if let Some(view) = self.get(*id) {
                    crate::scroll::behavior(surface, view, behavior);
                }
            }
            MountItem::Refreshing { id, refreshing } => {
                if let Some(view) = self.get(*id) {
                    crate::scroll::refreshing(surface, view, *refreshing);
                }
            }
            MountItem::Canvas { id, drawing } => {
                if let Some(view) = self.get(*id) {
                    crate::canvas::set(surface, view, drawing.clone());
                    self.draw_when_shown(view, Damage::All);
                }
            }
            MountItem::Shader { id, layer } => {
                if let Some(view) = self.get(*id) {
                    crate::shader::set(surface, view, layer.clone());
                }
            }
            MountItem::Gestures { id, values } => {
                if let Some(view) = self.get(*id) {
                    crate::input::gestures(surface, view, *values);
                }
            }
            MountItem::ViewProps { id, props } => {
                let Some(view) = self.get(*id) else { return };
                view.state_mut().props = props.clone();
                crate::input::props_changed(surface, view);
                crate::access::props_changed(view);
            }
            MountItem::LayoutAnimation(a) => self.layout_animation = Some(*a),
            MountItem::Input { id, config } => {
                if let Some(view) = self.get(*id) {
                    crate::text_input::config(surface, view, config);
                }
            }
            MountItem::InputValue { id, text, count } => {
                if let Some(view) = self.get(*id) {
                    crate::text_input::value(view, text, *count);
                }
            }
            MountItem::InputContent { id, content } => {
                if let Some(view) = self.get(*id) {
                    crate::text_input::content(view, *content);
                }
            }
            MountItem::ScreenConfig { id, config } => {
                if let Some(view) = self.get(*id) {
                    crate::navigation::screen_config(surface, view, config);
                }
            }
            MountItem::ModalConfig { id, config } => {
                if let Some(view) = self.get(*id) {
                    crate::navigation::modal_config(surface, view, config);
                }
            }
            MountItem::NativeProps { id, props } => {
                if let Some(view) = self.get(*id) {
                    crate::native::props(surface, view, props);
                }
            }
            MountItem::Component { id, name } => {
                if let Some(view) = self.get(*id) {
                    crate::native::component(surface, view, name);
                }
            }
            MountItem::ComponentProps { id, props } => {
                if let Some(view) = self.get(*id) {
                    crate::native::component_props(surface, view, props);
                }
            }
            MountItem::Command { id, command } => self.command(surface, *id, command),
            // The animator's (`Surface::apply`).
            MountItem::Animation(_)
            | MountItem::AnimationBindings { .. }
            | MountItem::ComponentPropSlots { .. } => {}
        }
    }

    /// Takes `view` out of its parent (a modal is dismissed).
    pub(crate) fn detach(&self, surface: &Rc<Surface>, view: &NshView) {
        match view.kind() {
            ViewKind::Modal => return crate::navigation::dismiss(surface, view),
            ViewKind::HeaderItem => return crate::navigation::header_item_removed(surface, view),
            _ => {}
        }
        let parent = view.parent_view();
        if view.has_parent() {
            view.unparent();
        }
        if let Some(parent) = parent
            && parent.kind() == ViewKind::Stack
        {
            crate::navigation::stack_changed(surface, &parent, None);
        }
    }

    fn command(&self, surface: &Rc<Surface>, id: NodeId, command: &Command) {
        let view = self.views.get(&id);
        match command {
            // An input edits; a pressable takes the keyboard focus as Tab
            // gives it.
            Command::Focus => match view {
                Some(view) if view.kind() == ViewKind::Input => crate::text_input::focus(view),
                Some(view) if id != ROOT_ID => crate::input::focus_control(surface, view),
                _ => {}
            },
            Command::Blur => match view {
                Some(view) if view.kind() == ViewKind::Input => crate::text_input::blur(view),
                Some(view) if id != ROOT_ID => {
                    crate::input::blur_control(surface, view);
                    crate::text_input::blur_all(surface);
                }
                _ => crate::text_input::blur_all(surface),
            },
            Command::SetSelection { start, end } => {
                if let Some(view) = view {
                    crate::text_input::set_selection(view, *start, *end);
                }
            }
            Command::ScrollTo { y, animated, .. }
                if view.is_some_and(|v| v.kind() == ViewKind::Input) =>
            {
                crate::text_input::scroll_to(view.expect("an input"), *y, *animated);
            }
            Command::ScrollTo { .. }
            | Command::ScrollToEnd { .. }
            | Command::ScrollToIndex { .. }
            | Command::ZoomTo { .. } => {
                if let Some(view) = view {
                    crate::scroll::command(surface, view, command);
                }
            }
            // Here, with the view at hand: `Surface::view` would borrow the
            // mounter again while a batch is applied.
            Command::AccessibilityFocus => {
                if let Some(view) = view {
                    crate::access::focus(view);
                }
            }
            Command::ImageInfo { .. } => crate::image::command(surface, id, command),
            Command::SetJsResponder { .. } | Command::ClearJsResponder => {
                crate::input::command(surface, id, command);
            }
            Command::Reload
            | Command::GoBack
            | Command::GoForward
            | Command::StopLoading
            | Command::InjectJavaScript(_)
            | Command::PostMessage(_)
            | Command::Seek(_)
            | Command::Native { .. } => {
                if let Some(view) = view {
                    crate::native::command(surface, view, command);
                }
            }
            other => crate::system::command(surface, id, other),
        }
    }
}
