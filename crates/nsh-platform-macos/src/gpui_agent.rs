//! The gpui-agent control plane (feature `gpui-agent`): an in-process
//! `AgentHost` so the `gpui-agent` CLI, recipes and MCP can drive a live
//! app — `snapshot`, `click`, `type`, `key`, `screenshot`, `assert`.
//!
//! Opt-in: [`install`] does nothing unless `GPUI_AGENT` is set
//! (`gpui_agent::security::from_env` enforces the token / insecure rules
//! and refuses release builds). The TCP server runs on its own thread and
//! posts each request on an [`AgentMailbox`]; a repeating timer on the
//! main queue drains it, so views are only ever touched on the main
//! thread.
//!
//! The semantic tree is what VoiceOver gets: each view's [`access::Spec`]
//! (role, label, value, states) becomes a [`UiNode`], identified by its
//! `testID` or `n<node>`; layout boxes that are no element are skipped
//! but walked through. Input is posted to the app as the system posts it
//! ([`crate::synth`]), the same path the `nsh test` agent takes
//! ([`crate::agent`]), so press handlers, text input and the keymap all
//! see real events.

use std::{rc::Rc, time::Duration};

use gpui_agent::{
    AgentMailbox, DispatchResult,
    protocol::{
        AssertSpec, DeliveryMode, HelloInfo, Op, PROTOCOL_VERSION, PlatformKind, Request, Response,
    },
    server,
    tree::{Bounds as TreeBounds, UiNode, UiTree},
};
use nsh_core::EventKind;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_foundation::NSRect;

use crate::{
    access,
    access::{Ax, Value},
    surface::Surface,
    synth,
    view::NshView,
};

/// How often the main queue drains the request mailbox.
const DRAIN: Duration = Duration::from_millis(15);
/// How long a TCP thread waits for its request to be answered.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Starts the server for `surface`, when `GPUI_AGENT` asks for one.
///
/// Called for every `AppView`; a `None` config or a failed bind only
/// logs — the app runs without the agent either way.
pub(crate) fn install(surface: &Rc<Surface>) {
    let config = match gpui_agent::security::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => return,
        Err(err) => {
            log::warn!("gpui-agent: {err}");
            return;
        }
    };
    let mailbox = AgentMailbox::new();
    let app = gpui_agent::discovery::detect_identity().executable;
    match server::spawn_mailbox_named(
        config.addr,
        config.token,
        mailbox.clone(),
        REQUEST_TIMEOUT,
        &app,
    ) {
        Ok((addr, _)) => {
            log::info!("gpui-agent: serving '{app}' on {addr}");
            schedule(surface.key(), mailbox);
        }
        Err(err) => log::warn!("gpui-agent: cannot bind {}: {err}", config.addr),
    }
}

/// Re-arms the drain on the main queue while the surface lives.
fn schedule(key: u64, mailbox: AgentMailbox) {
    crate::input::later(DRAIN.as_millis() as u64, move || {
        let Some(surface) = Surface::with_key(key) else {
            return;
        };
        for request in mailbox.take() {
            let response = handle(&surface, &request.request);
            request.reply(response);
        }
        schedule(key, mailbox);
    });
}

/// Answers one request, on the main thread.
fn handle(surface: &Rc<Surface>, request: &Request) -> Response {
    let id = &request.id;
    let result = match &request.op {
        Op::Hello => {
            let mut resp = Response::ok(id);
            resp.hello = Some(hello());
            return resp;
        }
        Op::Snapshot { .. } => {
            let mut resp = Response::ok(id);
            let tree = snapshot(surface);
            resp.tree = Some(tree);
            return resp;
        }
        Op::Revision => {
            // No revision tracking: every snapshot is rebuilt.
            return Response::ok(id);
        }
        Op::Wait { .. } => Ok(DispatchResult::empty()),
        Op::Click {
            target,
            delivery,
            expect_fingerprint,
        } => click(surface, target, *delivery, *expect_fingerprint),
        Op::Type {
            target,
            text,
            delivery,
            expect_fingerprint,
        } => with_focused(surface, target, *delivery, *expect_fingerprint, |window| {
            synth::type_text(window, text);
        }),
        Op::SetValue {
            target,
            value,
            expect_fingerprint,
        } => with_focused(
            surface,
            target,
            DeliveryMode::Semantic,
            *expect_fingerprint,
            |window| {
                synth::key(window, "cmd+a");
                synth::type_text(window, value);
            },
        ),
        Op::Key {
            target,
            key,
            delivery,
            expect_fingerprint,
        } => with_focused(surface, target, *delivery, *expect_fingerprint, |window| {
            synth::key(window, key);
        }),
        Op::Keybinding { binding, .. } => with_window(surface, |window| {
            synth::key(window, binding);
        }),
        Op::Assert { spec } => assert_spec(surface, spec),
        Op::Invoke { name, args } => match name.as_str() {
            "drag" => drag(surface, args),
            "hover" => hover(surface, args),
            "press" => press(surface, args),
            "scroll" => scroll(surface, args),
            _ => Err(format!("unknown invoke: {name}")),
        },
        Op::Screenshot { path, mode, .. } => screenshot(surface, path.as_deref(), *mode),
        Op::Shutdown => with_window(surface, |window| window.close()),
        Op::Keybindings => Ok(DispatchResult::json(serde_json::json!([]))),
        Op::WaitUntil { .. } => Err("wait_until is served by the server".into()),
    };
    match result {
        Ok(DispatchResult { value }) => {
            let mut resp = Response::ok(id);
            resp.result = value;
            resp
        }
        Err(error) => Response::err(id, error),
    }
}

fn hello() -> HelloInfo {
    let mut hello = HelloInfo {
        protocol: PROTOCOL_VERSION,
        app: gpui_agent::discovery::detect_identity().executable,
        platform: PlatformKind::Desktop,
        os: gpui_agent::protocol::host_os(),
        ready: true,
        deliveries: vec![DeliveryMode::Semantic, DeliveryMode::Virtual],
        auth: Default::default(),
        bundle_id: None,
        executable: None,
        features: vec![],
    };
    gpui_agent::discovery::fill_hello_identity(&mut hello);
    hello
}

// --- Snapshot ---------------------------------------------------------------

/// The semantic tree of `surface`: every view that is an accessibility
/// element, in subview order, as VoiceOver would list it.
fn snapshot(surface: &Rc<Surface>) -> UiTree {
    let mut nodes = Vec::new();
    let window = surface.root.ns().window();
    if let Some(window) = window.as_deref() {
        for child in surface.root.child_views() {
            collect(&child, window, &mut nodes);
        }
    }
    UiTree {
        app: gpui_agent::discovery::detect_identity().executable,
        platform: PlatformKind::Desktop,
        ready: true,
        nodes,
        rev: None,
    }
}

/// What `view` is to the tree a snapshot walks, as `Spec` says it is to
/// VoiceOver.
enum Place {
    /// `hidden`: the subtree with it.
    Hidden,
    /// `grouped`: one element; its children are not exposed.
    Grouped(UiNode),
    /// An element; its element children are exposed.
    Node(UiNode),
    /// No element (a plain layout box): VoiceOver goes through to its
    /// children, and so do we.
    Pass,
}

/// Adds `view` and its element descendants to `nodes`, flattened: an
/// element's children list holds its own element children, so agents see
/// containment without the layout boxes between. Hidden subtrees and a
/// grouped element's children are the accessibility tree's exclusions,
/// so they are the snapshot's too.
fn collect(view: &NshView, window: &NSWindow, nodes: &mut Vec<UiNode>) {
    match place(view, window) {
        Place::Hidden => {}
        Place::Grouped(mut node) => {
            node.children = Vec::new();
            nodes.push(node);
        }
        Place::Node(mut node) => {
            for child in view.child_views() {
                collect(&child, window, &mut node.children);
            }
            nodes.push(node);
        }
        Place::Pass => {
            for child in view.child_views() {
                collect(&child, window, nodes);
            }
        }
    }
}

/// [`Place`] for `view`, building its `UiNode` when it is an element.
fn place(view: &NshView, window: &NSWindow) -> Place {
    let (spec, test_id, text_input) = {
        let s = view.state();
        let a = &s.props.accessibility;
        let pressable =
            s.events.contains(EventKind::Press) || s.events.contains(EventKind::LongPress);
        let text = s.text.as_ref().map(|t| t.text.text.as_str());
        (
            access::spec_for(a, s.kind, pressable, text, s.style.hidden),
            s.props.test_id.clone(),
            s.input.is_some(),
        )
    };
    if spec.hidden {
        return Place::Hidden;
    }
    if spec.role == Ax::Generic {
        return Place::Pass;
    }
    let id = test_id
        .as_deref()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("n{}", view.id()));
    let mut node = UiNode::new(
        id,
        role_name(spec.role),
        spec.label.clone().unwrap_or_default(),
    );
    node.enabled = !spec.disabled;
    node.focused = text_input
        && window
            .firstResponder()
            .and_then(|r| r.downcast::<NSView>().ok())
            .is_some_and(|v| v.isDescendantOf(view.ns()));
    node.visible = true;
    node.value = value_text(&spec.value);
    node.checked = match spec.value {
        Value::Checked(v) => Some(v != 0),
        _ => None,
    };
    node.bounds = in_window(view.ns(), view.ns().bounds(), window)
        .map(|b| TreeBounds {
            x: b.origin.x.0,
            y: b.origin.y.0,
            w: b.size.width.0,
            h: b.size.height.0,
        })
        .unwrap_or_default();
    if spec.selected {
        node.states.push("selected".into());
    }
    if let Some(expanded) = spec.expanded {
        node.states.push(if expanded {
            "expanded".into()
        } else {
            "collapsed".into()
        });
    }
    if spec.grouped {
        node.states.push("grouped".into());
        return Place::Grouped(node);
    }
    Place::Node(node)
}

/// `view`'s node when it is an element (grouped or not).
fn element(view: &NshView, window: &NSWindow) -> Option<UiNode> {
    match place(view, window) {
        Place::Node(node) | Place::Grouped(node) => Some(node),
        _ => None,
    }
}

fn role_name(role: Ax) -> &'static str {
    match role {
        Ax::Generic => "generic",
        Ax::Button => "button",
        Ax::Link => "link",
        Ax::Text => "text",
        Ax::Image => "image",
        Ax::Group => "group",
        Ax::Heading => "heading",
        Ax::Slider => "slider",
        Ax::CheckBox => "checkbox",
        Ax::Switch => "switch",
        Ax::Radio => "radio",
        Ax::RadioGroup => "radiogroup",
        Ax::ComboBox => "combobox",
        Ax::Menu => "menu",
        Ax::MenuBar => "menubar",
        Ax::MenuItem => "menuitem",
        Ax::Progress => "progress",
        Ax::ScrollBar => "scrollbar",
        Ax::SpinButton => "spinbutton",
        Ax::Tab => "tab",
        Ax::TabList => "tablist",
        Ax::Toolbar => "toolbar",
        Ax::List => "list",
        Ax::Grid => "grid",
        Ax::Search => "search",
        Ax::TextField => "textfield",
        Ax::Alert => "alert",
        Ax::Status => "status",
        Ax::Timer => "timer",
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::None => None,
        Value::Text(text) => Some(text.clone()),
        Value::Checked(v) => Some(v.to_string()),
        Value::Range { now, text, .. } => text
            .as_deref()
            .map(str::to_owned)
            .or(now.map(|n| n.to_string())),
    }
}

// --- Targets ----------------------------------------------------------------

/// The view `target` names: a `testID`, a `n<node>` id, or — so agents
/// can act without the app naming every control — a node's name.
fn resolve(surface: &Rc<Surface>, target: &str) -> Option<NshView> {
    let mut by_name = None;
    let mut stack = surface.root.child_views();
    while let Some(view) = stack.pop() {
        let (hit, named, prune) = {
            let s = view.state();
            let a = &s.props.accessibility;
            let pressable =
                s.events.contains(EventKind::Press) || s.events.contains(EventKind::LongPress);
            let text = s.text.as_ref().map(|t| t.text.text.as_str());
            let spec = access::spec_for(a, s.kind, pressable, text, s.style.hidden);
            // What the snapshot excludes cannot be pressed, so it cannot
            // be found either: a hidden view's subtree is skipped whole.
            if spec.hidden {
                (false, false, true)
            } else if s.props.test_id.as_deref() == Some(target) || format!("n{}", s.id) == target {
                (true, false, false)
            } else {
                (
                    false,
                    spec.role != Ax::Generic && spec.label.as_deref() == Some(target),
                    false,
                )
            }
        };
        if hit {
            return Some(view);
        }
        if !prune {
            stack.extend(view.child_views());
        }
        if named && by_name.is_none() {
            by_name = Some(view);
        }
    }
    by_name
}

/// Fingerprint gate: the node the client saw must still be this one.
fn stale(surface: &Rc<Surface>, target: &str, expect: Option<u64>) -> Option<String> {
    let expect = expect?;
    let window = surface.root.ns().window()?;
    let view = resolve(surface, target)?;
    let node = element(&view, &window)?;
    let got = node.fingerprint(None);
    (got != expect).then(|| format!("stale_target: {target}"))
}

// --- Input ------------------------------------------------------------------

/// `rect` of view `ns` in the window's content, from its top left; `None`
/// when the view is not in the window or hidden. Same maths as the `nsh
/// test` agent's (`crate::agent`).
fn in_window(
    ns: &NSView,
    rect: NSRect,
    window: &NSWindow,
) -> Option<nsh_core::geometry::Bounds<nsh_core::geometry::Points>> {
    use nsh_core::geometry::{Bounds, Point, Points, Size};
    if ns.window().is_none() || ns.isHiddenOrHasHiddenAncestor() {
        return None;
    }
    let rect = ns.convertRect_toView(rect, None);
    let height = window.contentView()?.frame().size.height;
    Some(Bounds::new(
        Point::new(
            Points(rect.origin.x as f32),
            Points((height - rect.origin.y - rect.size.height) as f32),
        ),
        Size::new(
            Points(rect.size.width as f32),
            Points(rect.size.height as f32),
        ),
    ))
}

/// Makes the app active, as `crate::agent::MacNative::activate` does.
fn activate(window: &NSWindow) {
    let app = objc2_app_kit::NSApplication::sharedApplication(MainThreadMarker::from(window));
    if !app.isActive() {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

/// A user's press at `(x, y)` in window content points: hover, down, up.
fn press_at(window: &NSWindow, x: f64, y: f64) {
    activate(window);
    synth::mouse(window, NSEventType::MouseMoved, x, y, 0);
    synth::mouse(window, NSEventType::LeftMouseDown, x, y, 1);
    synth::mouse(window, NSEventType::LeftMouseUp, x, y, 1);
}

fn with_window(surface: &Rc<Surface>, f: impl FnOnce(&NSWindow)) -> Result<DispatchResult, String> {
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    f(&window);
    Ok(DispatchResult::empty())
}

/// Resolves `target`, checks the fingerprint gate, then presses its
/// centre — the click a user gives — and runs `f` while the app has
/// focus. `delivery` is accepted for either mode: this host's one path
/// posts real events.
fn with_focused(
    surface: &Rc<Surface>,
    target: &str,
    _delivery: DeliveryMode,
    expect_fingerprint: Option<u64>,
    f: impl FnOnce(&NSWindow),
) -> Result<DispatchResult, String> {
    if let Some(err) = stale(surface, target, expect_fingerprint) {
        return Err(err);
    }
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let Some(view) = resolve(surface, target) else {
        return Err(format!("target_gone: {target}"));
    };
    let Some(bounds) = in_window(view.ns(), view.ns().bounds(), &window) else {
        return Err(format!(
            "in_viewport_unavailable: {target} is not in the window"
        ));
    };
    let (x, y) = (
        f64::from(bounds.origin.x.0) + f64::from(bounds.size.width.0) / 2.0,
        f64::from(bounds.origin.y.0) + f64::from(bounds.size.height.0) / 2.0,
    );
    press_at(&window, x, y);
    f(&window);
    Ok(DispatchResult::empty())
}

fn click(
    surface: &Rc<Surface>,
    target: &str,
    delivery: DeliveryMode,
    expect_fingerprint: Option<u64>,
) -> Result<DispatchResult, String> {
    with_focused(surface, target, delivery, expect_fingerprint, |_| {})
}

/// `invoke("drag", {target, dx, dy, expect_fingerprint?})` — a user's
/// drag on `target`: hover, down at its centre, a run of dragged steps
/// to centre + `(dx, dy)` (pan gestures need a travel, not one jump),
/// up at the end. Points are content points, top-left origin.
fn drag(surface: &Rc<Surface>, args: &serde_json::Value) -> Result<DispatchResult, String> {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let dx = args.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let dy = args.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let expect = args.get("expect_fingerprint").and_then(|v| v.as_u64());
    if let Some(err) = stale(surface, target, expect) {
        return Err(err);
    }
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let (x0, y0) = point_of(surface, &window, args)?;
    activate(&window);
    synth::mouse(&window, NSEventType::MouseMoved, x0, y0, 0);
    synth::mouse(&window, NSEventType::LeftMouseDown, x0, y0, 1);
    const STEPS: usize = 6;
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        synth::mouse(
            &window,
            NSEventType::LeftMouseDragged,
            x0 + dx * t,
            y0 + dy * t,
            1,
        );
    }
    synth::mouse(&window, NSEventType::LeftMouseUp, x0 + dx, y0 + dy, 1);
    Ok(DispatchResult::empty())
}

/// The point an invoke's args name: explicit `x`/`y` content points
/// (top-left origin), `target`'s centre, or the window's centre.
fn point_of(
    surface: &Rc<Surface>,
    window: &NSWindow,
    args: &serde_json::Value,
) -> Result<(f64, f64), String> {
    if let (Some(x), Some(y)) = (
        args.get("x").and_then(|v| v.as_f64()),
        args.get("y").and_then(|v| v.as_f64()),
    ) {
        return Ok((x, y));
    }
    if let Some(target) = args.get("target").and_then(|v| v.as_str()) {
        let Some(view) = resolve(surface, target) else {
            return Err(format!("target_gone: {target}"));
        };
        let Some(bounds) = in_window(view.ns(), view.ns().bounds(), window) else {
            return Err(format!(
                "in_viewport_unavailable: {target} is not in the window"
            ));
        };
        return Ok((
            f64::from(bounds.origin.x.0) + f64::from(bounds.size.width.0) / 2.0,
            f64::from(bounds.origin.y.0) + f64::from(bounds.size.height.0) / 2.0,
        ));
    }
    let b = surface.root.ns().bounds();
    Ok((b.size.width / 2.0, b.size.height / 2.0))
}

/// `invoke("press", {x?, y?, target?})` — a user's press at a point:
/// hover, down, up. For content with no accessible node to click, such
/// as a mini app's inner views.
fn press(surface: &Rc<Surface>, args: &serde_json::Value) -> Result<DispatchResult, String> {
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let (x, y) = point_of(surface, &window, args)?;
    press_at(&window, x, y);
    Ok(DispatchResult::empty())
}

/// `invoke("hover", {x?, y?, target?})` — a pointer move with no press:
/// the hover a user gives a row to light its action bar. Points are
/// content points, top-left origin; `target` centres on it.
fn hover(surface: &Rc<Surface>, args: &serde_json::Value) -> Result<DispatchResult, String> {
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let (x, y) = point_of(surface, &window, args)?;
    activate(&window);
    // `postEvent` drops `MouseMoved` when the window's
    // `acceptsMouseMovedEvents` is off (tracking areas opt real moves in,
    // not posted ones): dispatch it through the input path itself.
    if let Some(event) = synth::mouse_event(
        &window,
        NSEventType::MouseMoved,
        x,
        y,
        0,
        NSEventModifierFlags::empty(),
    ) {
        crate::input::event(surface, &event);
    }
    Ok(DispatchResult::empty())
}

/// `invoke("scroll", {dx, dy, x?, y?, target?})` — a trackpad scroll
/// gesture of `(dx, dy)` points at `x,y` (or `target`'s centre, or the
/// window's): began, a run of changed deltas a gesture apart, ended —
/// the same shape `agent::scroll` feeds the headless driver.
fn scroll(surface: &Rc<Surface>, args: &serde_json::Value) -> Result<DispatchResult, String> {
    let dx = args.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let dy = args.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let (x, y) = point_of(surface, &window, args)?;
    activate(&window);
    const STEP: f64 = 24.0;
    let gesture = dx != 0.0 || dy != 0.0;
    let frames = ((dx.abs().max(dy.abs()) / STEP).ceil() as u32).max(1);
    if gesture {
        synth::wheel_with(&window, x, y, 0, 0, Some(1), 0, synth::timestamp());
    }
    let (mut sent_x, mut sent_y) = (0i32, 0i32);
    for i in 1..=frames {
        let t = i as f64 / f64::from(frames);
        let (to_x, to_y) = ((dx * t).round() as i32, (dy * t).round() as i32);
        if !gesture || (to_x == sent_x && to_y == sent_y) {
            continue;
        }
        synth::wheel_with(
            &window,
            x,
            y,
            to_x - sent_x,
            to_y - sent_y,
            Some(2),
            0,
            synth::timestamp(),
        );
        (sent_x, sent_y) = (to_x, to_y);
    }
    if gesture {
        synth::wheel_with(&window, x, y, 0, 0, Some(4), 0, synth::timestamp());
    }
    // `momentum`: that many more points as the system's coasting momentum
    // (phase events a trackpad sends after the gesture ends); left
    // running — no end — so a press can be checked against a live glide.
    // It runs along the gesture's axis, or vertically on its own sign.
    let momentum = args.get("momentum").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if momentum != 0.0 {
        // `dx=dy=0` continues momentum already running (changed deltas
        // only, as the system sends until it decays).
        if gesture {
            synth::wheel_with(&window, x, y, 0, 0, None, 1, synth::timestamp());
        }
        let horizontal = dx != 0.0 && dy == 0.0;
        let frames = ((momentum.abs() / STEP).ceil() as u32).max(1);
        let (mut sent_x, mut sent_y) = (0i32, 0i32);
        for i in 1..=frames {
            let t = i as f64 / f64::from(frames);
            let v = (momentum * t).round() as i32;
            let (to_x, to_y) = if horizontal { (v, 0) } else { (0, v) };
            synth::wheel_with(
                &window,
                x,
                y,
                to_x - sent_x,
                to_y - sent_y,
                None,
                2,
                synth::timestamp(),
            );
            (sent_x, sent_y) = (to_x, to_y);
        }
    }
    Ok(DispatchResult::empty())
}

fn screenshot(
    surface: &Rc<Surface>,
    path: Option<&str>,
    mode: gpui_agent::protocol::ScreenshotMode,
) -> Result<DispatchResult, String> {
    if mode.is_scrolled() {
        return Err(gpui_agent::scroll_capture::scroll_unavailable(
            "scrolled capture is not implemented on this host",
        ));
    }
    let Some(path) = path else {
        return Err("screenshot needs a path".into());
    };
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let png = synth::shot(&window)?;
    std::fs::write(path, &png).map_err(|e| format!("cannot write {path}: {e}"))?;
    Ok(DispatchResult::json(serde_json::json!({ "path": path })))
}

fn assert_spec(surface: &Rc<Surface>, spec: &AssertSpec) -> Result<DispatchResult, String> {
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    let node = resolve(surface, &spec.target).and_then(|v| element(&v, &window));
    if spec.exists == Some(false) {
        return if node.is_none() {
            Ok(DispatchResult::empty())
        } else {
            Err(format!("assert failed: '{}' exists", spec.target))
        };
    }
    let Some(node) = node else {
        return Err(format!("assert failed: '{}' not found", spec.target));
    };
    for (field, want, got) in [
        ("name", spec.name.as_deref(), Some(node.name.as_str())),
        ("role", spec.role.as_deref(), Some(node.role.as_str())),
        ("value", spec.value.as_deref(), node.value.as_deref()),
    ] {
        if let Some(want) = want
            && got != Some(want)
        {
            return Err(format!(
                "assert failed: '{}' {field} = {:?}, expected {want:?}",
                spec.target,
                got.unwrap_or("<none>")
            ));
        }
    }
    if let Some(want) = spec.checked
        && node.checked != Some(want)
    {
        return Err(format!("assert failed: '{}' checked mismatch", spec.target));
    }
    if let Some(want) = spec.visible
        && node.visible != want
    {
        return Err(format!("assert failed: '{}' visible mismatch", spec.target));
    }
    if let Some(want) = spec.in_viewport {
        let has = node.bounds.has_area();
        if has != want {
            return Err(format!(
                "assert failed: '{}' in_viewport mismatch",
                spec.target
            ));
        }
    }
    Ok(DispatchResult::empty())
}
