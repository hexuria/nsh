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

use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    rc::Rc,
    time::{Duration, Instant},
};

use gpui_agent::{
    AgentMailbox, DispatchResult,
    mailbox::MailboxRequest,
    protocol::{
        AssertSpec, DeliveryMode, HelloInfo, Op, PROTOCOL_VERSION, PlatformKind, Request, Response,
    },
    server,
    tree::{Bounds as TreeBounds, UiNode, UiTree},
};
use nsh_core::{
    EventKind, ViewKind,
    geometry::{Bounds, Point, Points, Size},
};
use objc2::{MainThreadMarker, Message};
use objc2_app_kit::{NSEventModifierFlags, NSEventType, NSView, NSWindow};
use objc2_foundation::NSRect;

use crate::{
    access,
    access::{Ax, Value},
    surface::Surface,
    synth,
    view::NshView,
};

/// How often the main queue drains the request mailbox and steps the
/// action in flight: about a display frame.
const DRAIN: Duration = Duration::from_millis(15);
/// How long a TCP thread waits for its request to be answered.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// How many drains an action waits for its target to be actionable
/// (visible, enabled, reached by the press) before it fails: about a
/// second, as long as a screen's transition takes to end.
const ACTIONABLE_TRIES: u32 = 60;
/// How many drains a press waits for the focus to land in the field it
/// pressed: the responder moves on the engine's reply.
const FOCUS_TRIES: u32 = 50;
/// How many drains in a row the app must be quiet to have settled: about
/// 100 ms, as `nsh test`'s agent waits (`nsh_testing::agent`), which
/// covers the engine's turn from an event to the batch it makes.
const QUIET_DRAINS: u32 = 7;
/// How long an action waits for the app to settle at most: an app that
/// animates for ever still gets its reply.
const SETTLE: Duration = Duration::from_secs(2);
/// The longest `wait` a client may ask for: below the server's request
/// timeout, so the reply still reaches it.
const MAX_WAIT: Duration = Duration::from_secs(25);

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
            schedule(surface.key(), mailbox, VecDeque::new());
        }
        Err(err) => log::warn!("gpui-agent: cannot bind {}: {err}", config.addr),
    }
}

/// Re-arms the drain on the main queue while the surface lives.
///
/// Requests that read the app are answered at once; actions are jobs,
/// stepped once per drain, one at a time and in order. Between two drains
/// the main queue is free, so the app handles the posted input, applies
/// the engine's batches and runs its animations: an action never blocks
/// the main thread while it waits for the app.
fn schedule(key: u64, mailbox: AgentMailbox, mut jobs: VecDeque<Job>) {
    crate::input::later(DRAIN.as_millis() as u64, move || {
        let Some(surface) = Surface::with_key(key) else {
            for job in jobs {
                let id = job.request.request.id.clone();
                job.request.reply(Response::err(id, "the window is gone"));
            }
            return;
        };
        for request in mailbox.take() {
            match start(&surface, &request.request) {
                Start::Done(response) => request.reply(response),
                Start::Act(action) => jobs.push_back(Job {
                    request,
                    action: Some(action),
                    phase: Phase::Locate {
                        tries: 0,
                        revealed: false,
                    },
                }),
                Start::Settle(timeout) => jobs.push_back(Job {
                    request,
                    action: None,
                    phase: Phase::Settle(Settle::new(timeout, true)),
                }),
            }
        }
        while let Some(job) = jobs.front_mut() {
            let Some(result) = step(&surface, job) else {
                break;
            };
            if let Some(job) = jobs.pop_front() {
                let id = job.request.request.id.clone();
                job.request.reply(response(&id, result));
            }
        }
        schedule(key, mailbox, jobs);
    });
}

/// What a request starts: an answer now, an action to step, or a wait
/// for the app to settle.
enum Start {
    Done(Response),
    Act(Action),
    Settle(Duration),
}

fn response(id: &str, result: Result<DispatchResult, String>) -> Response {
    match result {
        Ok(DispatchResult { value }) => {
            let mut resp = Response::ok(id);
            resp.result = value;
            resp
        }
        Err(error) => Response::err(id, error),
    }
}

/// Answers one request, on the main thread, or makes it an action.
fn start(surface: &Rc<Surface>, request: &Request) -> Start {
    let id = &request.id;
    let result = match &request.op {
        Op::Hello => {
            let mut resp = Response::ok(id);
            resp.hello = Some(hello());
            return Start::Done(resp);
        }
        Op::Snapshot {
            since,
            fingerprints,
            ..
        } => {
            let mut resp = Response::ok(id);
            let tree = snapshot(surface);
            if *fingerprints == Some(true) {
                resp.fingerprints = Some(tree.fingerprint_map());
            }
            resp.rev = tree.rev;
            match since {
                Some(rev) if *rev == revision(&tree) => {
                    resp.result = Some(serde_json::json!({ "unchanged": true }));
                }
                // No earlier trees are kept to diff against: the whole one.
                Some(_) => {
                    resp.result = Some(serde_json::json!({ "full": true }));
                    resp.tree = Some(tree);
                }
                None => resp.tree = Some(tree),
            }
            return Start::Done(resp);
        }
        Op::Revision => {
            let mut resp = Response::ok(id);
            resp.rev = snapshot(surface).rev;
            return Start::Done(resp);
        }
        Op::Wait { timeout_ms } => {
            let timeout = timeout_ms.map_or(SETTLE, Duration::from_millis);
            return Start::Settle(timeout.min(MAX_WAIT));
        }
        Op::Click {
            target,
            expect_fingerprint,
            ..
        } => return act(surface, id, target, *expect_fingerprint, Gesture::Press),
        Op::Type {
            target,
            text,
            expect_fingerprint,
            ..
        } => {
            return act(
                surface,
                id,
                target,
                *expect_fingerprint,
                Gesture::Type(text.clone()),
            );
        }
        Op::SetValue {
            target,
            value,
            expect_fingerprint,
        } => {
            return act(
                surface,
                id,
                target,
                *expect_fingerprint,
                Gesture::SetValue(value.clone()),
            );
        }
        Op::Key {
            target,
            key,
            expect_fingerprint,
            ..
        } => {
            return act(
                surface,
                id,
                target,
                *expect_fingerprint,
                Gesture::Key(key.clone()),
            );
        }
        Op::Keybinding { binding, .. } => with_window(surface, |window| {
            synth::key(window, binding);
        }),
        Op::Assert { spec } => assert_spec(surface, spec),
        Op::Invoke { name, args } => match invoke_action(name, args) {
            Some(Ok(action)) => {
                let expect = args.get("expect_fingerprint").and_then(|v| v.as_u64());
                if let Some(target) = &action.target
                    && let Some(err) = stale(surface, target, expect)
                {
                    Err(err)
                } else {
                    return Start::Act(action);
                }
            }
            Some(Err(err)) => Err(err),
            None => Err(format!("unknown invoke: {name}")),
        },
        Op::Screenshot { path, mode, .. } => screenshot(surface, path.as_deref(), *mode),
        Op::Shutdown => with_window(surface, |window| window.close()),
        Op::Keybindings => Ok(DispatchResult::json(serde_json::json!([]))),
        Op::WaitUntil { .. } => Err("wait_until is served by the server".into()),
    };
    Start::Done(response(id, result))
}

/// An action on `target`, after the fingerprint gate. `delivery` is
/// accepted for either mode: this host's one path posts real events.
fn act(
    surface: &Rc<Surface>,
    id: &str,
    target: &str,
    expect_fingerprint: Option<u64>,
    gesture: Gesture,
) -> Start {
    if let Some(err) = stale(surface, target, expect_fingerprint) {
        return Start::Done(Response::err(id, err));
    }
    Start::Act(Action {
        target: Some(target.to_owned()),
        at: None,
        gesture,
    })
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
            collect(surface, &child, window, &mut nodes);
        }
    }
    let mut tree = UiTree {
        app: gpui_agent::discovery::detect_identity().executable,
        platform: PlatformKind::Desktop,
        ready: true,
        nodes,
        rev: None,
    };
    tree.rev = Some(revision(&tree));
    tree
}

/// The tree's revision: a hash of what it says, so an equal revision is
/// an equal tree. `wait_until` skips its assert while the revision
/// stands, which a counter of batches would get wrong (a scroll moves
/// nodes in and out of the viewport without one).
fn revision(tree: &UiTree) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&tree.nodes)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

/// Where `view` lands in the semantic tree: hidden views exclude their
/// whole subtree, a grouped view is one element (its children are not
/// exposed), a plain layout box passes through to its descendants, and
/// anything else is a node.
enum Place {
    Hidden,
    Grouped(UiNode),
    Node(UiNode),
    Pass,
}

/// Adds `view`'s exposed element descendants to `nodes` under its own
/// node, or flattens them when `view` is a pass-through box. Hidden
/// views are pruned whole: nothing under them is ever exposed.
fn collect(surface: &Surface, view: &NshView, window: &NSWindow, nodes: &mut Vec<UiNode>) {
    match place(view, window) {
        Place::Hidden => {}
        Place::Grouped(mut node) => {
            measure(surface, view, window, &mut node);
            node.children = Vec::new();
            nodes.push(node);
        }
        Place::Node(mut node) => {
            measure(surface, view, window, &mut node);
            for child in view.child_views() {
                collect(surface, &child, window, &mut node.children);
            }
            nodes.push(node);
        }
        Place::Pass => {
            for child in view.child_views() {
                collect(surface, &child, window, nodes);
            }
        }
    }
}

/// The semantic place of `view`: VoiceOver goes through a plain layout
/// box to its children, and so do we. Its node has no geometry yet
/// ([`measure`]): finding a target needs none.
fn place(view: &NshView, window: &NSWindow) -> Place {
    let spec = spec_of(view);
    let (test_id, text_input) = {
        let s = view.state();
        (s.props.test_id.clone(), s.input.is_some())
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
    node.focused = text_input && focused_in(view, window);
    node.value = value_text(&spec.value);
    node.checked = match spec.value {
        Value::Checked(v) => Some(v != 0),
        _ => None,
    };
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

/// Fills in where `view`'s node is: its bounds in the window's content,
/// and whether it can be seen, by the rule actions follow
/// ([`visibility`]). A node a scroll view has out of its viewport is
/// visible, not in the viewport: scrolling brings it in.
fn measure(surface: &Surface, view: &NshView, window: &NSWindow, node: &mut UiNode) {
    node.visible = visibility(surface, view, window).visible();
    node.bounds = in_window(view.ns(), view.ns().bounds(), window)
        .map(|b| TreeBounds {
            x: b.origin.x.0,
            y: b.origin.y.0,
            w: b.size.width.0,
            h: b.size.height.0,
        })
        .unwrap_or_default();
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
        Ax::Option => "option",
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

/// A node of the snapshot: its view, its node (measured) and its parent
/// node's id, which the node's fingerprint covers.
struct Found {
    view: NshView,
    node: UiNode,
    parent: Option<String>,
}

/// The view `target` names: a `testID`, a `n<node>` id, or — so agents
/// can act without the app naming every control — a node's name.
fn resolve(surface: &Rc<Surface>, target: &str) -> Option<NshView> {
    find(surface, target).map(|found| found.view)
}

/// The node `target` names, among exactly the nodes the snapshot shows,
/// in its order: ids win over names, and a name goes to its first node.
/// A view the snapshot leaves out (hidden, under a grouped element, a
/// plain box even with a `testID`) cannot be targeted.
fn find(surface: &Rc<Surface>, target: &str) -> Option<Found> {
    let window = surface.root.ns().window()?;
    let mut by_name = None;
    let found = surface
        .root
        .child_views()
        .iter()
        .find_map(|child| find_in(child, &window, target, None, &mut by_name));
    let mut found = found.or(by_name)?;
    measure(surface, &found.view, &window, &mut found.node);
    Some(found)
}

fn find_in(
    view: &NshView,
    window: &NSWindow,
    target: &str,
    parent: Option<&str>,
    by_name: &mut Option<Found>,
) -> Option<Found> {
    let (node, children) = match place(view, window) {
        Place::Hidden => return None,
        Place::Pass => {
            return view
                .child_views()
                .iter()
                .find_map(|child| find_in(child, window, target, parent, by_name));
        }
        Place::Grouped(node) => (node, false),
        Place::Node(node) => (node, true),
    };
    let found = |node: UiNode| Found {
        view: view.clone(),
        node,
        parent: parent.map(str::to_owned),
    };
    if node.id == target {
        return Some(found(node));
    }
    let id = node.id.clone();
    if by_name.is_none() && node.name == target {
        *by_name = Some(found(node));
    }
    if !children {
        return None;
    }
    view.child_views()
        .iter()
        .find_map(|child| find_in(child, window, target, Some(&id), by_name))
}

/// Fingerprint gate: the node the client saw must still be this one. The
/// fingerprint is the snapshot's ([`UiTree::fingerprint_map`]): its
/// parent's id is part of it.
fn stale(surface: &Rc<Surface>, target: &str, expect: Option<u64>) -> Option<String> {
    let expect = expect?;
    let found = find(surface, target)?;
    let got = found.node.fingerprint(found.parent.as_deref());
    (got != expect).then(|| format!("stale_target: {target}"))
}

/// `view`'s accessibility, as [`place`] reads it.
fn spec_of(view: &NshView) -> access::Spec {
    let s = view.state();
    let pressable = s.events.contains(EventKind::Press) || s.events.contains(EventKind::LongPress);
    let text = s.text.as_ref().map(|t| t.text.text.as_str());
    access::spec_for(
        &s.props.accessibility,
        s.kind,
        pressable,
        text,
        s.style.hidden,
    )
}

/// How a client names `view` in an error: its id, and its name if it has
/// one.
fn describe(view: &NshView) -> String {
    let id = view
        .state()
        .props
        .test_id
        .as_deref()
        .map_or_else(|| format!("n{}", view.id()), str::to_owned);
    match spec_of(view).label {
        Some(label) if !label.is_empty() => format!("{id} {label:?}"),
        _ => id,
    }
}

// --- Actions ----------------------------------------------------------------

/// A request in flight ([`schedule`]): an action, or a wait.
struct Job {
    request: MailboxRequest,
    action: Option<Action>,
    phase: Phase,
}

/// What an input request does, and where.
#[derive(Debug, PartialEq)]
struct Action {
    /// The node it acts on; `None` acts at `at` or the window's middle.
    target: Option<String>,
    /// An explicit point (content points from the top left), which wins
    /// over `target`: for content no node names, such as a mini app's.
    at: Option<(f64, f64)>,
    gesture: Gesture,
}

/// The input of an [`Action`].
#[derive(Debug, PartialEq)]
enum Gesture {
    /// Hover, down, up.
    Press,
    /// A press, then the text once the field has the focus.
    Type(String),
    /// A press, then Select All and the text.
    SetValue(String),
    /// A press, then a key.
    Key(String),
    /// A pointer move with no press.
    Hover,
    /// A press-drag-release of `(dx, dy)` points.
    Drag { dx: f64, dy: f64 },
    /// A trackpad gesture of `(dx, dy)` points and `momentum` more.
    Scroll { dx: f64, dy: f64, momentum: f64 },
}

impl Gesture {
    /// It sends keys after its press, which must have focused the target:
    /// keys must not land on whatever held the focus before.
    fn types(&self) -> bool {
        matches!(
            self,
            Gesture::Type(_) | Gesture::SetValue(_) | Gesture::Key(_)
        )
    }
}

/// Where a [`Job`] is.
enum Phase {
    /// Waiting for the target to be actionable; `revealed`: its scroll
    /// views were scrolled to bring it in.
    Locate { tries: u32, revealed: bool },
    /// Pressed; waiting for the focus to land in the target.
    Focus { tries: u32 },
    /// The input is given; waiting for the app to settle.
    Settle(Settle),
}

/// The wait for the app to settle: no batches coming or waiting, no
/// native animation, no display frame JS asked for, for
/// [`QUIET_DRAINS`] drains in a row, until a deadline. An action replies
/// once it is over, so the next snapshot sees what it did.
struct Settle {
    until: Instant,
    /// The batch count the quiet drains saw.
    batches: Option<u64>,
    quiet: u32,
    /// The reply says whether it settled (`wait`); an action's says so
    /// only when it did not.
    report: bool,
}

impl Settle {
    fn new(timeout: Duration, report: bool) -> Settle {
        Settle {
            until: Instant::now() + timeout,
            batches: None,
            quiet: 0,
            report,
        }
    }

    /// One drain's look at the app's activity ([`Surface::activity`]):
    /// `Some(settled)` once it is over.
    fn poll(&mut self, (batches, busy): (u64, bool), now: Instant) -> Option<bool> {
        if busy || self.batches != Some(batches) {
            self.quiet = 0;
            self.batches = Some(batches);
        } else {
            self.quiet += 1;
        }
        if self.quiet >= QUIET_DRAINS {
            return Some(true);
        }
        (now >= self.until).then_some(false)
    }

    fn result(&self, settled: bool) -> DispatchResult {
        if self.report || !settled {
            DispatchResult::json(serde_json::json!({ "settled": settled }))
        } else {
            DispatchResult::empty()
        }
    }
}

/// The action an `invoke` names (`None`: no such invoke).
///
/// - `press {x?, y?, target?}`: a press.
/// - `hover {x?, y?, target?}`: a pointer move with no press, the hover
///   a user gives a row to light its action bar.
/// - `drag {target?, x?, y?, dx, dy}`: a press-drag-release.
/// - `scroll {dx, dy, momentum?, x?, y?, target?}`: a trackpad gesture.
fn invoke_action(name: &str, args: &serde_json::Value) -> Option<Result<Action, String>> {
    let num = |key: &str| args.get(key).and_then(|v| v.as_f64());
    let gesture = match name {
        "press" => Gesture::Press,
        "hover" => Gesture::Hover,
        "drag" => Gesture::Drag {
            dx: num("dx").unwrap_or(0.0),
            dy: num("dy").unwrap_or(0.0),
        },
        "scroll" => Gesture::Scroll {
            dx: num("dx").unwrap_or(0.0),
            dy: num("dy").unwrap_or(0.0),
            momentum: num("momentum").unwrap_or(0.0),
        },
        _ => return None,
    };
    let at = num("x").zip(num("y"));
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    Some(Ok(Action {
        target,
        at,
        gesture,
    }))
}

/// Steps `job` once; `Some` when it is done.
///
/// Before its input an action waits until its target is actionable: shown
/// (in the window, not hidden, not clipped away; scrolled into view first
/// when a scroll view has it out of sight), enabled, and the one the
/// press reaches. It retries for [`ACTIONABLE_TRIES`] drains, so a screen
/// still sliding in or an overlay fading out does not fail it, then fails
/// with why: a press that would land elsewhere returns an error instead
/// of reporting a press nothing received.
fn step(surface: &Rc<Surface>, job: &mut Job) -> Option<Result<DispatchResult, String>> {
    let Some(window) = surface.root.ns().window() else {
        return Some(Err("the window is gone".into()));
    };
    let action = match (&mut job.phase, &job.action) {
        (Phase::Settle(settle), _) => {
            let settled = settle.poll(surface.activity(), Instant::now())?;
            if !settled {
                log::info!("gpui-agent: the app did not settle; replying all the same");
            }
            return Some(Ok(settle.result(settled)));
        }
        (_, Some(action)) => action,
        (_, None) => return Some(Err("nothing to act on".into())),
    };
    let target = action.target.as_deref().unwrap_or("the window");
    match &mut job.phase {
        Phase::Locate { tries, revealed } => {
            let (x, y) = match locate(surface, &window, action) {
                Ok(point) => point,
                Err(Unready::Offscreen(view)) if !*revealed => {
                    *revealed = true;
                    crate::scroll::reveal(surface, &view);
                    return None;
                }
                Err(unready) => {
                    *tries += 1;
                    return (*tries >= ACTIONABLE_TRIES).then(|| Err(unready.error(target)));
                }
            };
            perform(surface, &window, &action.gesture, x, y);
            job.phase = if action.gesture.types() {
                // The press is in the event queue: the run loop handles it
                // before the next drain.
                Phase::Focus { tries: 0 }
            } else {
                Phase::Settle(Settle::new(SETTLE, false))
            };
            None
        }
        Phase::Focus { tries } => {
            let focused = action
                .target
                .as_deref()
                .and_then(|t| resolve(surface, t))
                .is_some_and(|view| focused_in(&view, &window));
            if !focused {
                *tries += 1;
                return (*tries >= FOCUS_TRIES).then(|| {
                    Err(format!(
                        "focus_missed: the press on {target} did not focus it"
                    ))
                });
            }
            match &action.gesture {
                Gesture::Type(text) => synth::type_text(&window, text),
                Gesture::SetValue(value) => {
                    synth::key(&window, "cmd+a");
                    synth::type_text(&window, value);
                }
                Gesture::Key(key) => synth::key(&window, key),
                _ => {}
            }
            job.phase = Phase::Settle(Settle::new(SETTLE, false));
            None
        }
        Phase::Settle(_) => None,
    }
}

// --- Actionability ----------------------------------------------------------

/// Why a target cannot take an action yet.
enum Unready {
    /// No node of the snapshot has this id or name.
    Gone,
    /// Hidden, empty, or clipped away by an ancestor that does not scroll.
    NotVisible,
    /// Out of a scroll view's viewport, still after scrolling it in.
    Offscreen(NshView),
    /// It, or an ancestor, is disabled.
    Disabled,
    /// The press would land on another view: this one.
    Occluded(String),
}

impl Unready {
    /// The error a client branches on: a code, a colon, the details.
    fn error(&self, target: &str) -> String {
        match self {
            Unready::Gone => format!("target_gone: {target}"),
            Unready::NotVisible => {
                format!("not_visible: {target} is hidden, empty or clipped away")
            }
            Unready::Offscreen(_) => {
                format!("offscreen: {target} is out of its scroll view's viewport")
            }
            Unready::Disabled => format!("disabled: {target}"),
            Unready::Occluded(by) => {
                format!("occluded_by: {by} (a press on {target} lands there)")
            }
        }
    }
}

/// Where `view` can be seen ([`visibility`]).
enum Visibility {
    /// This part of it shows (window content points, from the top left).
    Shown(Bounds<Points>),
    /// A scroll view has it out of its viewport: scrolling brings it in.
    Offscreen,
    /// Hidden, empty, or clipped away where scrolling does not help.
    Hidden,
}

impl Visibility {
    /// The snapshot's `visible`: it shows, or would once scrolled to.
    fn visible(&self) -> bool {
        !matches!(self, Visibility::Hidden)
    }

    /// The snapshot's `in_viewport`: some of it shows now.
    fn in_viewport(&self) -> bool {
        matches!(self, Visibility::Shown(_))
    }
}

/// Where `view` can be seen: not hidden up its chain, with an area, and
/// left of it by every clipping ancestor (scroll views at their current
/// offset) and the window's content.
fn visibility(surface: &Surface, view: &NshView, window: &NSWindow) -> Visibility {
    let chain = || std::iter::successors(Some(view.clone()), NshView::parent_view);
    let size = view.ns().frame().size;
    if size.width <= 0.0 || size.height <= 0.0 || chain().any(|v| v.state().style.hidden) {
        return Visibility::Hidden;
    }
    let cut = || {
        if chain().skip(1).any(|v| v.kind() == ViewKind::Scroll) {
            Visibility::Offscreen
        } else {
            Visibility::Hidden
        }
    };
    let Some(part) = view.shown_part() else {
        return cut();
    };
    let Some(part) = in_window(surface.root.ns(), part, window) else {
        return Visibility::Hidden;
    };
    let Some(content) = window.contentView().map(|v| v.frame().size) else {
        return Visibility::Hidden;
    };
    match within(part, content.width as f32, content.height as f32) {
        Some(part) => Visibility::Shown(part),
        None => cut(),
    }
}

/// What of `rect` lies in a `width` × `height` window content, if it has
/// an area.
fn within(rect: Bounds<Points>, width: f32, height: f32) -> Option<Bounds<Points>> {
    let x0 = rect.origin.x.0.max(0.0);
    let y0 = rect.origin.y.0.max(0.0);
    let x1 = (rect.origin.x.0 + rect.size.width.0).min(width);
    let y1 = (rect.origin.y.0 + rect.size.height.0).min(height);
    (x1 > x0 && y1 > y0).then(|| {
        Bounds::new(
            Point::new(Points(x0), Points(y0)),
            Size::new(Points(x1 - x0), Points(y1 - y0)),
        )
    })
}

/// The point `action` acts at, once its target is actionable.
fn locate(
    surface: &Rc<Surface>,
    window: &NSWindow,
    action: &Action,
) -> Result<(f64, f64), Unready> {
    if let Some(at) = action.at {
        return Ok(at);
    }
    let Some(target) = &action.target else {
        let b = surface.root.ns().bounds();
        return Ok((b.size.width / 2.0, b.size.height / 2.0));
    };
    let view = resolve(surface, target).ok_or(Unready::Gone)?;
    actionable(surface, window, &view)
}

/// The point to press `view` at — the middle of its shown part — when it
/// is shown, enabled and the press reaches it there.
fn actionable(
    surface: &Rc<Surface>,
    window: &NSWindow,
    view: &NshView,
) -> Result<(f64, f64), Unready> {
    let part = match visibility(surface, view, window) {
        Visibility::Shown(part) => part,
        Visibility::Offscreen => return Err(Unready::Offscreen(view.clone())),
        Visibility::Hidden => return Err(Unready::NotVisible),
    };
    if std::iter::successors(Some(view.clone()), NshView::parent_view).any(|v| spec_of(&v).disabled)
    {
        return Err(Unready::Disabled);
    }
    let (x, y) = (
        f64::from(part.origin.x.0 + part.size.width.0 / 2.0),
        f64::from(part.origin.y.0 + part.size.height.0 / 2.0),
    );
    match receiver(surface, window, x, y) {
        Receiver::View(ns) if ns.isDescendantOf(view.ns()) => Ok((x, y)),
        Receiver::View(ns) => Err(Unready::Occluded(owner(&ns))),
        Receiver::Nothing => Err(Unready::Occluded("nothing that takes the pointer".into())),
    }
}

/// What a press at a point reaches.
enum Receiver {
    /// The view it lands on: one of nsh's or a hosted AppKit view.
    View(objc2::rc::Retained<NSView>),
    Nothing,
}

/// What a press at `(x, y)` (content points from the top left) lands
/// on, as the window delivers it: AppKit's hit test of the content view,
/// and over the root nsh's own picking (`pointerEvents`, transforms,
/// views animating out), which the root's `hitTest:` hands the press to.
fn receiver(surface: &Surface, window: &NSWindow, x: f64, y: f64) -> Receiver {
    let Some(content) = window.contentView() else {
        return Receiver::Nothing;
    };
    let p = synth::to_window(window, x, y);
    let in_super = match crate::view::superview(&content) {
        Some(frame) => frame.convertPoint_fromView(p, None),
        None => p,
    };
    let Some(hit) = content.hitTest(in_super) else {
        return Receiver::Nothing;
    };
    let root = surface.root.ns();
    if &*hit != root {
        return Receiver::View(hit);
    }
    let r = root.convertPoint_fromView(p, None);
    match crate::input::view_at(surface, r.x as f32, r.y as f32) {
        Some(view) => Receiver::View(view.ns().retain()),
        None => Receiver::Nothing,
    }
}

/// Names the view a press landed on: the nsh view it is or is hosted by,
/// else its AppKit class.
fn owner(ns: &NSView) -> String {
    let mut current = Some(ns.retain());
    while let Some(v) = current {
        if let Some(view) = NshView::from_ns(&v) {
            return describe(&view);
        }
        current = crate::view::superview(&v);
    }
    format!("a {}", ns.class().name().to_string_lossy())
}

// --- Input ------------------------------------------------------------------

/// `rect` of view `ns` in the window's content, from its top left; `None`
/// when the view is not in the window or hidden. Same maths as the `nsh
/// test` agent's (`crate::agent`).
fn in_window(ns: &NSView, rect: NSRect, window: &NSWindow) -> Option<Bounds<Points>> {
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

fn with_window(surface: &Rc<Surface>, f: impl FnOnce(&NSWindow)) -> Result<DispatchResult, String> {
    let Some(window) = surface.root.ns().window() else {
        return Err("the window is gone".into());
    };
    f(&window);
    Ok(DispatchResult::empty())
}

/// `view` or a descendant of it is the responder.
fn focused_in(view: &NshView, window: &NSWindow) -> bool {
    window
        .firstResponder()
        .and_then(|r| r.downcast::<NSView>().ok())
        .is_some_and(|v| v.isDescendantOf(view.ns()))
}

/// Gives `gesture`'s pointer input at `(x, y)`, content points from the
/// top left.
fn perform(surface: &Rc<Surface>, window: &NSWindow, gesture: &Gesture, x: f64, y: f64) {
    activate(window);
    match *gesture {
        Gesture::Press | Gesture::Type(_) | Gesture::SetValue(_) | Gesture::Key(_) => {
            synth::mouse(window, NSEventType::MouseMoved, x, y, 0);
            synth::mouse(window, NSEventType::LeftMouseDown, x, y, 1);
            synth::mouse(window, NSEventType::LeftMouseUp, x, y, 1);
        }
        Gesture::Hover => hover(surface, window, x, y),
        Gesture::Drag { dx, dy } => drag(window, x, y, dx, dy),
        Gesture::Scroll { dx, dy, momentum } => scroll(window, x, y, dx, dy, momentum),
    }
}

/// A user's drag: hover, down, a run of dragged steps to `(dx, dy)` away
/// (pan gestures need a travel, not one jump), up at the end.
fn drag(window: &NSWindow, x0: f64, y0: f64, dx: f64, dy: f64) {
    synth::mouse(window, NSEventType::MouseMoved, x0, y0, 0);
    synth::mouse(window, NSEventType::LeftMouseDown, x0, y0, 1);
    const STEPS: usize = 6;
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        synth::mouse(
            window,
            NSEventType::LeftMouseDragged,
            x0 + dx * t,
            y0 + dy * t,
            1,
        );
    }
    synth::mouse(window, NSEventType::LeftMouseUp, x0 + dx, y0 + dy, 1);
}

/// A pointer move with no press.
fn hover(surface: &Rc<Surface>, window: &NSWindow, x: f64, y: f64) {
    // `postEvent` drops `MouseMoved` when the window's
    // `acceptsMouseMovedEvents` is off (tracking areas opt real moves in,
    // not posted ones): dispatch it through the input path itself.
    if let Some(event) = synth::mouse_event(
        window,
        NSEventType::MouseMoved,
        x,
        y,
        0,
        NSEventModifierFlags::empty(),
    ) {
        crate::input::event(surface, &event);
    }
}

/// A trackpad scroll gesture of `(dx, dy)` points at `(x, y)`: began, a
/// run of changed deltas a gesture apart, ended — the same shape
/// `agent::scroll` feeds the headless driver. `momentum`: that many more
/// points as the system's coasting momentum (phase events a trackpad
/// sends after the gesture ends), left running — no end — so a press can
/// be checked against a live glide. It runs along the gesture's axis, or
/// vertically on its own.
fn scroll(window: &NSWindow, x: f64, y: f64, dx: f64, dy: f64, momentum: f64) {
    const STEP: f64 = 24.0;
    let gesture = dx != 0.0 || dy != 0.0;
    let frames = ((dx.abs().max(dy.abs()) / STEP).ceil() as u32).max(1);
    if gesture {
        synth::wheel_with(window, x, y, 0, 0, Some(1), 0, synth::timestamp());
    }
    let (mut sent_x, mut sent_y) = (0i32, 0i32);
    for i in 1..=frames {
        let t = i as f64 / f64::from(frames);
        let (to_x, to_y) = ((dx * t).round() as i32, (dy * t).round() as i32);
        if !gesture || (to_x == sent_x && to_y == sent_y) {
            continue;
        }
        synth::wheel_with(
            window,
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
        synth::wheel_with(window, x, y, 0, 0, Some(4), 0, synth::timestamp());
    }
    if momentum != 0.0 {
        // `dx=dy=0` continues momentum already running (changed deltas
        // only, as the system sends until it decays).
        if gesture {
            synth::wheel_with(window, x, y, 0, 0, None, 1, synth::timestamp());
        }
        let horizontal = dx != 0.0 && dy == 0.0;
        let frames = ((momentum.abs() / STEP).ceil() as u32).max(1);
        let (mut sent_x, mut sent_y) = (0i32, 0i32);
        for i in 1..=frames {
            let t = i as f64 / f64::from(frames);
            let v = (momentum * t).round() as i32;
            let (to_x, to_y) = if horizontal { (v, 0) } else { (0, v) };
            synth::wheel_with(
                window,
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
    let found = find(surface, &spec.target);
    if spec.exists == Some(false) {
        return if found.is_none() {
            Ok(DispatchResult::empty())
        } else {
            Err(format!("assert failed: '{}' exists", spec.target))
        };
    }
    let Some(Found { view, node, .. }) = found else {
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
        let has = visibility(surface, &view, &window).in_viewport();
        if has != want {
            return Err(format!(
                "assert failed: '{}' in_viewport mismatch",
                spec.target
            ));
        }
    }
    Ok(DispatchResult::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invoke_with_a_point_acts_there_whatever_its_target() {
        let args = serde_json::json!({"x": 10.0, "y": 20.0, "target": "row", "dx": 5.0});
        let action = invoke_action("drag", &args).unwrap().unwrap();
        assert_eq!(action.at, Some((10.0, 20.0)));
        assert_eq!(action.target.as_deref(), Some("row"));
        assert_eq!(action.gesture, Gesture::Drag { dx: 5.0, dy: 0.0 });
        assert!(invoke_action("teleport", &args).is_none());
    }

    #[test]
    fn only_typing_gestures_wait_for_the_focus() {
        assert!(Gesture::Type("a".into()).types());
        assert!(Gesture::Key("Return".into()).types());
        assert!(!Gesture::Press.types());
        assert!(!Gesture::Hover.types());
    }

    #[test]
    fn a_rect_is_cut_to_the_window_content() {
        let rect = |x, y, w, h| {
            Bounds::new(
                Point::new(Points(x), Points(y)),
                Size::new(Points(w), Points(h)),
            )
        };
        assert_eq!(
            within(rect(-10.0, 790.0, 50.0, 40.0), 400.0, 800.0),
            Some(rect(0.0, 790.0, 40.0, 10.0))
        );
        assert_eq!(within(rect(0.0, 800.0, 50.0, 40.0), 400.0, 800.0), None);
        assert_eq!(within(rect(10.0, 10.0, 0.0, 40.0), 400.0, 800.0), None);
    }

    #[test]
    fn the_app_settles_after_quiet_drains_in_a_row() {
        let now = Instant::now();
        let mut settle = Settle::new(SETTLE, false);
        // The first look only learns the batch count.
        assert_eq!(settle.poll((3, false), now), None);
        for _ in 1..QUIET_DRAINS {
            assert_eq!(settle.poll((3, false), now), None);
        }
        assert_eq!(settle.poll((3, false), now), Some(true));
    }

    #[test]
    fn a_batch_or_an_animation_starts_the_quiet_over() {
        let now = Instant::now();
        let mut settle = Settle::new(SETTLE, false);
        for _ in 0..QUIET_DRAINS {
            settle.poll((3, false), now);
        }
        assert_eq!(settle.poll((4, false), now), None);
        assert_eq!(settle.poll((4, true), now), None);
        assert_eq!(settle.quiet, 0);
    }

    #[test]
    fn an_app_that_never_settles_gets_its_reply_at_the_deadline() {
        let mut settle = Settle::new(Duration::from_millis(10), false);
        let later = Instant::now() + Duration::from_secs(1);
        assert_eq!(settle.poll((1, true), later), Some(false));
        assert_eq!(
            settle.result(false).value,
            Some(serde_json::json!({ "settled": false }))
        );
        assert_eq!(settle.result(true).value, None);
        assert!(Settle::new(SETTLE, true).result(true).value.is_some());
    }

    #[test]
    fn equal_trees_have_equal_revisions() {
        let tree = |name: &str| UiTree {
            app: "app".into(),
            platform: PlatformKind::Desktop,
            ready: true,
            nodes: vec![UiNode::new("save", "button", name)],
            rev: None,
        };
        assert_eq!(revision(&tree("Save")), revision(&tree("Save")));
        assert_ne!(revision(&tree("Save")), revision(&tree("Saved")));
    }

    #[test]
    fn a_node_scrolled_out_is_visible_but_not_in_the_viewport() {
        let shown = Visibility::Shown(Bounds::new(
            Point::new(Points(0.0), Points(0.0)),
            Size::new(Points(10.0), Points(10.0)),
        ));
        assert!(shown.visible() && shown.in_viewport());
        assert!(Visibility::Offscreen.visible() && !Visibility::Offscreen.in_viewport());
        assert!(!Visibility::Hidden.visible() && !Visibility::Hidden.in_viewport());
    }

    #[test]
    fn unready_targets_fail_with_a_code_first() {
        assert_eq!(Unready::Gone.error("save"), "target_gone: save");
        assert!(
            Unready::NotVisible
                .error("save")
                .starts_with("not_visible: save")
        );
        assert_eq!(Unready::Disabled.error("save"), "disabled: save");
        let occluded = Unready::Occluded("n42 \"FPS\"".into()).error("close");
        assert!(
            occluded.starts_with("occluded_by: n42 \"FPS\""),
            "{occluded}"
        );
    }
}
