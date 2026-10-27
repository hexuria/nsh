//! Adapter from the shared Native Shell test agent to the GPUI-agent protocol.

#[cfg(feature = "dev-agent")]
use std::sync::Mutex;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[cfg(feature = "dev-agent")]
use gpui_agent::AgentServer;
pub use gpui_agent::PlatformKind;
use gpui_agent::{
    AgentHost, DispatchResult, HelloAuth, HelloInfo, KeybindingInfo, KeybindingScope, Op,
    PROTOCOL_VERSION, ScreenshotMode, UiNode, UiTree, role, scroll_capture::ScreenshotSpec,
};
use nsh_core::{
    NodeId,
    geometry::{Point, Points},
};
use nsh_testing::agent::{Agent, SemanticKeybinding, SemanticNode};

/// Native Shell host exposed through GPUI-agent.
#[non_exhaustive]
pub struct NshAgentHost {
    app: String,
    platform: PlatformKind,
    agent: Arc<Agent>,
    token_required: bool,
    screenshot_directory: Option<PathBuf>,
}

impl NshAgentHost {
    /// Creates a host. The shared `Agent` must use native-hit-test enforcement.
    pub fn new(
        app: impl Into<String>,
        platform: PlatformKind,
        agent: Arc<Agent>,
    ) -> Result<Self, String> {
        if !agent.requires_native_hit_test() {
            return Err("GPUI-agent requires native hit-test enforcement".into());
        }
        Ok(Self {
            app: app.into(),
            platform,
            agent,
            token_required: false,
            screenshot_directory: None,
        })
    }

    /// Confines screenshot output to a host-owned writable directory.
    pub fn with_screenshot_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.screenshot_directory = Some(directory.into());
        self
    }

    fn node_id(&self, target: &str) -> Result<NodeId, String> {
        self.agent.settle()?;
        let nodes = self.agent.semantic_nodes()?;
        let ids = stable_ids(&nodes);
        ids.iter()
            .find(|(_, id)| id.as_str() == target)
            .map(|(node, _)| *node)
            .ok_or_else(|| format!("target_gone: {target}"))
    }

    fn require_available(&self) -> Result<(), String> {
        if self.agent.native_available() {
            Ok(())
        } else {
            Err("attached native surface is unavailable".into())
        }
    }

    fn dispatch_available(&mut self, op: &Op) -> Result<DispatchResult, String> {
        match op {
            Op::Click { target, .. } => self
                .agent
                .press_node(self.node_id(target)?)
                .map(|()| DispatchResult::empty()),
            Op::Type { target, text, .. } => self
                .agent
                .type_node(self.node_id(target)?, text)
                .map(|()| DispatchResult::empty()),
            Op::SetValue { target, value, .. } => self
                .agent
                .set_value_node(self.node_id(target)?, value)
                .map(|()| DispatchResult::empty()),
            Op::Shutdown => Err(
                "shutdown is unsupported for an attached app; stop the host process instead".into(),
            ),
            Op::Key { target, key, .. } => self
                .agent
                .key_node(self.node_id(target)?, key)
                .map(|()| DispatchResult::empty()),
            Op::Invoke { name, args } => self.invoke(name, args),
            Op::Keybinding { .. } => {
                let bindings = self.keybindings();
                let entry =
                    gpui_agent::authorize_keybinding_op(op, &bindings, self.is_app_focused())?;
                self.agent
                    .fire_keybinding(&entry.id)
                    .map(|()| DispatchResult::empty())
            }
            _ => Err("operation is handled by the GPUI-agent dispatcher".into()),
        }
    }
}

impl AgentHost for NshAgentHost {
    fn hello(&self) -> HelloInfo {
        HelloInfo {
            protocol: PROTOCOL_VERSION,
            app: self.app.clone(),
            platform: self.platform,
            os: std::env::consts::OS.into(),
            ready: self.agent.native_available()
                && self.agent.has_rendered_batch().unwrap_or(false)
                && self.agent.semantic_nodes().is_ok()
                && self.agent.native_available(),
            deliveries: vec![gpui_agent::DeliveryMode::Semantic],
            auth: if self.token_required {
                HelloAuth::Required
            } else {
                HelloAuth::None
            },
            bundle_id: None,
            executable: None,
            features: Vec::new(),
        }
    }

    fn snapshot(&self) -> UiTree {
        if !self.agent.native_available()
            || !matches!(self.agent.has_rendered_batch(), Ok(true))
            || self.agent.native_laid_out().is_err()
        {
            return UiTree {
                app: self.app.clone(),
                platform: self.platform,
                ready: false,
                nodes: Vec::new(),
                rev: None,
            };
        }
        let Ok(nodes) = self.agent.semantic_nodes() else {
            return UiTree {
                app: self.app.clone(),
                platform: self.platform,
                ready: false,
                nodes: Vec::new(),
                rev: None,
            };
        };
        let index = SnapshotIndex::new(&nodes, self.agent.as_ref());
        let roots = nodes
            .iter()
            .filter(|node| {
                node.parent
                    .is_none_or(|parent| !index.known.contains_key(&parent))
            })
            .map(|node| ui_node(node, &index, self.agent.as_ref()))
            .collect();
        if !self.agent.native_available() {
            return UiTree {
                app: self.app.clone(),
                platform: self.platform,
                ready: false,
                nodes: Vec::new(),
                rev: None,
            };
        }
        UiTree {
            app: self.app.clone(),
            platform: self.platform,
            ready: true,
            nodes: roots,
            rev: None,
        }
    }

    fn dispatch(&mut self, op: &Op) -> Result<DispatchResult, String> {
        self.require_available()?;
        let result = self.dispatch_available(op);
        if self.agent.native_available() {
            result
        } else {
            Err("attached native surface is unavailable".into())
        }
    }

    fn screenshot(&self, spec: ScreenshotSpec<'_>) -> Result<DispatchResult, String> {
        self.require_available()?;
        spec.validate_request()?;
        if spec.mode != ScreenshotMode::Viewport {
            return Err(gpui_agent::screenshot_unavailable(
                "scrolled capture is unsupported",
            ));
        }
        let path = gpui_agent::require_screenshot_path(spec.path)?;
        let png = self.agent.native_screenshot()?;
        self.require_available()?;
        match &self.screenshot_directory {
            Some(directory) => gpui_agent::write_png_in(path, &png, directory),
            None => gpui_agent::write_png(path, &png),
        }
        .map(DispatchResult::json)
    }

    fn revision(&self) -> Option<u64> {
        None
    }

    fn keybindings(&self) -> Vec<KeybindingInfo> {
        if !self.agent.native_available() {
            return Vec::new();
        }
        self.agent
            .semantic_keybindings()
            .unwrap_or_default()
            .iter()
            .map(keybinding_info)
            .collect()
    }

    fn is_app_focused(&self) -> bool {
        self.agent.app_focused()
    }
}

impl NshAgentHost {
    fn invoke(&self, name: &str, args: &serde_json::Value) -> Result<DispatchResult, String> {
        let target = |name: &str| {
            args.get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("invoke `{name}` requires a string target"))
                .and_then(|target| self.node_id(target))
        };
        match name {
            "press" => {
                if args.get("target").is_some() {
                    self.agent.press_node(target("target")?)?;
                } else {
                    self.agent.press_at(
                        optional_point(args)?.ok_or("invoke `press` needs x and y or target")?,
                    )?;
                }
                Ok(DispatchResult::empty())
            }
            "scroll" => {
                let dx = number_arg(args, "dx")?;
                let dy = number_arg(args, "dy")?;
                if number_arg(args, "momentum")? != 0.0 {
                    return Err("scroll momentum is unsupported".into());
                }
                let delta = Point::new(Points(dx), Points(dy));
                if args.get("target").is_some() {
                    let id = target("target")?;
                    if !self
                        .agent
                        .semantic_nodes()?
                        .iter()
                        .any(|node| node.id == id && node.scroll)
                    {
                        return Err(format!("node {id} is not scrollable"));
                    }
                    self.agent.scroll_node(id, delta)?;
                } else {
                    self.agent.scroll_at(
                        optional_point(args)?.ok_or("invoke `scroll` needs x and y or target")?,
                        delta,
                    )?;
                }
                Ok(DispatchResult::empty())
            }
            "hover" => {
                if args.get("target").is_some() {
                    self.agent.hover_node(target("target")?)?;
                } else {
                    self.agent.hover_at(
                        optional_point(args)?.ok_or("invoke `hover` needs x and y or target")?,
                    )?;
                }
                Ok(DispatchResult::empty())
            }
            "drag" => {
                if args.get("from").is_some() || args.get("to").is_some() {
                    self.agent.drag_nodes(target("from")?, target("to")?)?;
                } else if args.get("target").is_some() {
                    let id = target("target")?;
                    let dx = number_arg(args, "dx")?;
                    let dy = number_arg(args, "dy")?;
                    self.agent.drag_node_by(
                        id,
                        optional_point(args)?,
                        Point::new(Points(dx), Points(dy)),
                    )?;
                } else {
                    let origin =
                        optional_point(args)?.ok_or("invoke `drag` needs x and y or target")?;
                    let dx = number_arg(args, "dx")?;
                    let dy = number_arg(args, "dy")?;
                    self.agent.drag_at(
                        origin,
                        Point::new(Points(origin.x.0 + dx), Points(origin.y.0 + dy)),
                    )?;
                }
                Ok(DispatchResult::empty())
            }
            "viewport" => {
                let (width, height) = viewport_size(args)?;
                self.agent.resize_viewport(Points(width), Points(height))?;
                Ok(DispatchResult::json(serde_json::json!({
                    "width": self.agent.native_viewport().width.0,
                    "height": self.agent.native_viewport().height.0,
                })))
            }
            _ => Err(format!("invoke `{name}` is unsupported")),
        }
    }
}

fn viewport_size(args: &serde_json::Value) -> Result<(f32, f32), String> {
    let preset = args.get("preset").and_then(serde_json::Value::as_str);
    let size = match preset {
        Some("phone") => Some((390.0, 844.0)),
        Some("tablet") => Some((820.0, 1180.0)),
        Some("laptop") => Some((1280.0, 800.0)),
        Some("desktop") => Some((1440.0, 900.0)),
        Some(other) => return Err(format!("unknown viewport preset `{other}`")),
        None => None,
    };
    let (width, height) = size.unwrap_or_else(|| {
        (
            args.get("width")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default() as f32,
            args.get("height")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default() as f32,
        )
    });
    if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
        return Err("viewport width and height must be finite and positive".into());
    }
    Ok((width, height))
}

fn number_arg(args: &serde_json::Value, name: &str) -> Result<f32, String> {
    let Some(value) = args.get(name) else {
        return Ok(0.0);
    };
    let number = value
        .as_f64()
        .ok_or_else(|| format!("invoke `{name}` must be a finite number"))?;
    let number = number as f32;
    if !number.is_finite() {
        return Err(format!("invoke `{name}` must be a finite number"));
    }
    Ok(number)
}

fn optional_point(args: &serde_json::Value) -> Result<Option<Point<Points>>, String> {
    match (args.get("x"), args.get("y")) {
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Ok(Some(Point::new(
            Points(number_arg(args, "x")?),
            Points(number_arg(args, "y")?),
        ))),
        _ => Err("invoke coordinates require both x and y".into()),
    }
}

fn stable_ids(nodes: &[SemanticNode]) -> HashMap<NodeId, String> {
    let bases: Vec<String> = nodes
        .iter()
        .map(|node| {
            node.test_id
                .as_deref()
                .filter(|test_id| !test_id.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("n{}", node.id))
        })
        .collect();
    let mut counts = HashMap::new();
    for base in &bases {
        *counts.entry(base.clone()).or_insert(0usize) += 1;
    }
    // The `base#id` form of every node: a base equal to another node's
    // gets the form too. (Never a node's own: `base#id` is longer than
    // `base`, so the other node's base differs as well.)
    let generated: HashSet<String> = nodes
        .iter()
        .zip(&bases)
        .map(|(node, base)| format!("{base}#{}", node.id))
        .collect();
    let mut used = HashSet::new();
    nodes
        .iter()
        .zip(bases.iter().cloned())
        .map(|(node, base)| {
            let generated_by_other = generated.contains(&base);
            let mut id = if counts[&base] == 1 && !generated_by_other {
                base.clone()
            } else {
                format!("{base}#{}", node.id)
            };
            if !used.insert(id.clone()) {
                id = format!("{id}#{}", node.id);
                let mut suffix = 2;
                while !used.insert(id.clone()) {
                    id = format!("{id}#{suffix}");
                    suffix += 1;
                }
            }
            (node.id, id)
        })
        .collect()
}

/// One semantic tree's lookups for a snapshot, built once: nodes,
/// children and native bounds by id, so that the snapshot neither asks the
/// scene for the tree again nor searches it per node.
struct SnapshotIndex<'a> {
    known: HashMap<NodeId, &'a SemanticNode>,
    children: HashMap<NodeId, Vec<&'a SemanticNode>>,
    bounds: HashMap<NodeId, nsh_core::geometry::Bounds<Points>>,
    ids: HashMap<NodeId, String>,
    viewport: nsh_core::geometry::Size<Points>,
}

impl<'a> SnapshotIndex<'a> {
    fn new(nodes: &'a [SemanticNode], agent: &Agent) -> Self {
        let mut children: HashMap<NodeId, Vec<&SemanticNode>> = HashMap::new();
        for node in nodes {
            if let Some(parent) = node.parent {
                children.entry(parent).or_default().push(node);
            }
        }
        Self {
            known: nodes.iter().map(|node| (node.id, node)).collect(),
            children,
            bounds: nodes
                .iter()
                .filter_map(|node| Some((node.id, agent.node_native_bounds(node)?)))
                .collect(),
            ids: stable_ids(nodes),
            viewport: agent.native_viewport(),
        }
    }
}

fn ui_node(node: &SemanticNode, index: &SnapshotIndex<'_>, agent: &Agent) -> UiNode {
    let mut name = node
        .label
        .clone()
        .or_else(|| node.value.clone())
        .or_else(|| node.test_id.clone())
        .unwrap_or_default();
    if node.grouped {
        name.push_str(" [grouped]");
    }
    if let Some(hint) = &node.hint {
        if !name.is_empty() {
            name.push_str(" — ");
        }
        name.push_str("hint: ");
        name.push_str(hint);
    }
    let role = match node.role.as_str() {
        "button" => role::BUTTON,
        "textbox" => role::TEXTBOX,
        "scroll" => role::SCROLL,
        "text" => role::NOTE,
        _ => node.role.as_str(),
    };
    let mut result = UiNode::new(
        index
            .ids
            .get(&node.id)
            .cloned()
            .unwrap_or_else(|| format!("n{}", node.id)),
        role,
        name,
    );
    result.enabled = node.enabled;
    // The scene already masks a masked input's value; the native text is
    // masked the same way: a bullet per character, never the text.
    let masked = node.states.iter().any(|state| state == "masked");
    result.value = agent
        .native_input_text(node.id)
        .map(|text| {
            if masked {
                "\u{2022}".repeat(text.chars().count())
            } else {
                text
            }
        })
        .or_else(|| node.value.clone());
    if let Some(bounds) = index.bounds.get(&node.id) {
        result.bounds = gpui_agent::Bounds {
            x: bounds.origin.x.0,
            y: bounds.origin.y.0,
            w: bounds.size.width.0,
            h: bounds.size.height.0,
        };
    }
    result.visible = visible_in_viewport(node, index);
    result.states.clone_from(&node.states);
    if let Some(hint) = &node.hint {
        result.states.push(format!("hint: {hint}"));
    }
    if node.grouped {
        result.states.push("grouped".into());
    }
    if !node.enabled && !result.states.iter().any(|state| state == "disabled") {
        result.states.push("disabled".into());
    }
    if node.selected && !result.states.iter().any(|state| state == "selected") {
        result.states.push("selected".into());
    }
    result.children = index
        .children
        .get(&node.id)
        .into_iter()
        .flatten()
        .map(|child| ui_node(child, index, agent))
        .collect();
    result
}

fn visible_in_viewport(node: &SemanticNode, index: &SnapshotIndex<'_>) -> bool {
    let Some(bounds) = index.bounds.get(&node.id) else {
        return false;
    };
    let viewport = index.viewport;
    let (mut x0, mut y0, mut x1, mut y1) = (0.0_f32, 0.0_f32, viewport.width.0, viewport.height.0);
    let mut parent = node.parent;
    while let Some(id) = parent {
        let Some(ancestor) = index.known.get(&id) else {
            return false;
        };
        if ancestor.clipping {
            let Some(clip) = index.bounds.get(&id) else {
                return false;
            };
            x0 = x0.max(clip.origin.x.0);
            y0 = y0.max(clip.origin.y.0);
            x1 = x1.min(clip.origin.x.0 + clip.size.width.0);
            y1 = y1.min(clip.origin.y.0 + clip.size.height.0);
        }
        parent = ancestor.parent;
    }
    x0 < x1
        && y0 < y1
        && bounds.origin.x.0 < x1
        && x0 < bounds.origin.x.0 + bounds.size.width.0
        && bounds.origin.y.0 < y1
        && y0 < bounds.origin.y.0 + bounds.size.height.0
}

fn keybinding_info(binding: &SemanticKeybinding) -> KeybindingInfo {
    let chord = binding.chord.replace('+', "-");
    let scope = if binding.menu_path.is_some() {
        KeybindingScope::Global
    } else {
        KeybindingScope::Focused
    };
    let label = binding
        .label
        .as_deref()
        .unwrap_or(&binding.id)
        .to_ascii_lowercase();
    let dangerous_label = [
        "quit", "exit", "delete", "remove", "reset", "restart", "sign out",
    ]
    .iter()
    .any(|word| label.contains(word));
    let mut entry = KeybindingInfo::new(&binding.id, chord, scope, dangerous_label);
    entry.dangerous |= gpui_agent::binding_is_dangerous(&entry);
    entry
}

/// Validates the worker's bind address and token without opening a socket.
pub fn validate_worker_config(addr: SocketAddr, token: &str) -> Result<(), String> {
    gpui_agent::ensure_loopback(addr).map_err(|error| error.to_string())?;
    if token.trim().is_empty() {
        return Err("worker token must not be empty".into());
    }
    Ok(())
}

/// Bound worker lifecycle. Dropping it requests shutdown without blocking.
#[non_exhaustive]
pub struct WorkerHandle {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
}

impl WorkerHandle {
    /// The address selected by the operating system.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

/// Starts a loopback-only worker with upstream GPUI-agent authentication.
#[cfg(feature = "dev-agent")]
pub fn spawn_worker(
    addr: SocketAddr,
    token: String,
    mut host: NshAgentHost,
) -> Result<WorkerHandle, String> {
    validate_worker_config(addr, &token)?;
    host.token_required = true;
    let server = AgentServer::bind(addr, Some(token)).map_err(|error| error.to_string())?;
    let local_addr = server.local_addr().map_err(|error| error.to_string())?;
    let shutdown = server.shutdown_handle();
    let thread = std::thread::Builder::new()
        .name("nsh-gpui-agent".into())
        .spawn(move || server.serve_host(Arc::new(Mutex::new(host))))
        .map_err(|error| error.to_string())?;
    drop(thread);
    Ok(WorkerHandle {
        addr: local_addr,
        shutdown,
    })
}

/// Validates the upstream environment configuration without starting a worker.
#[cfg(feature = "dev-agent")]
pub fn worker_config_from_env() -> Result<Option<(SocketAddr, String)>, String> {
    if std::env::var("GPUI_AGENT").ok().as_deref() != Some("1") {
        return Ok(None);
    }
    let Some(config) = gpui_agent::from_env().map_err(|error| error.to_string())? else {
        return Err("GPUI_AGENT=1 requires GPUI_AGENT_TOKEN and GPUI_AGENT_ADDR".into());
    };
    let token = config
        .token
        .filter(|token| !token.is_empty())
        .ok_or("GPUI_AGENT=1 requires a nonempty GPUI_AGENT_TOKEN")?;
    validate_worker_config(config.addr, &token)?;
    Ok(Some((config.addr, token)))
}

/// Starts the opt-in development worker from the upstream environment config.
#[cfg(feature = "dev-agent")]
pub fn start_from_env(host: NshAgentHost) -> Result<Option<WorkerHandle>, String> {
    let Some((addr, token)) = worker_config_from_env()? else {
        return Ok(None);
    };
    spawn_worker(addr, token, host).map(Some)
}

#[cfg(test)]
mod tests {
    use nsh_core::geometry::{Bounds, Size};
    use nsh_testing::agent::{Native, Scene};

    use super::*;

    struct SnapshotScene(
        Result<Vec<SemanticNode>, String>,
        u64,
        /// How many times the semantic tree was built.
        Arc<std::sync::atomic::AtomicUsize>,
    );

    impl Scene for SnapshotScene {
        fn activity(&self) -> Result<nsh_testing::agent::Activity, String> {
            Ok(nsh_testing::agent::Activity {
                batches: self.1,
                since_last: Some(std::time::Duration::ZERO),
            })
        }

        fn semantic_tree(&self) -> Result<Vec<SemanticNode>, String> {
            self.2.fetch_add(1, Ordering::Relaxed);
            self.0.clone()
        }

        fn take_errors(&self) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }

        fn pick(
            &self,
            _: &str,
            _: nsh_dev::protocol::PickTarget,
        ) -> Result<nsh_testing::Pick, String> {
            Err("unused test scene method".into())
        }

        fn scroll_ancestors(&self, _: NodeId) -> Result<Vec<(NodeId, bool)>, String> {
            Ok(Vec::new())
        }

        fn long_press_delay(&self, _: NodeId) -> Result<Option<f32>, String> {
            Ok(None)
        }

        fn horizontal(&self, _: NodeId) -> Result<bool, String> {
            Ok(false)
        }

        fn texts(&self) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }

        fn count(&self, _: &str) -> Result<usize, String> {
            Ok(0)
        }

        fn matches(&self, _: &str) -> Result<bool, String> {
            Ok(false)
        }

        fn enabled(&self, _: &str) -> Result<bool, String> {
            Ok(false)
        }

        fn span_style(&self, _: &str) -> Result<nsh_dev::protocol::SpanStyle, String> {
            Err("unused test scene method".into())
        }

        fn input_id(&self, _: &str) -> Result<Option<NodeId>, String> {
            Ok(None)
        }

        fn dialogs(&self) -> Result<Vec<nsh_core::Dialog>, String> {
            Ok(Vec::new())
        }

        fn pending_dialog(&self) -> Result<Option<nsh_core::Dialog>, String> {
            Ok(None)
        }

        fn answered(&self) -> Result<(), String> {
            Ok(())
        }

        fn menu(&self) -> Result<Option<Vec<nsh_core::desktop::MenuItem>>, String> {
            Ok(None)
        }

        fn menu_closed(&self) -> Result<(), String> {
            Ok(())
        }

        fn clipboard(&self) -> Result<Option<String>, String> {
            Ok(None)
        }

        fn storage(&self) -> Result<std::collections::BTreeMap<String, String>, String> {
            Ok(std::collections::BTreeMap::new())
        }

        fn set_viewport(&self, _: Size<Points>) -> Result<(), String> {
            Ok(())
        }

        fn span(&self, _: NodeId) -> Result<Option<(NodeId, u32, u32)>, String> {
            Ok(None)
        }

        fn move_clock(&self, _: nsh_dev::protocol::ClockChange) -> Result<(), String> {
            Ok(())
        }

        fn send_host_event(&self, _: &str, _: serde_json::Value) -> Result<(), String> {
            Ok(())
        }

        fn opened_urls(&self) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
    }

    /// Available, and node 3's native input text.
    /// Available, the text of a masked input, not laid out.
    struct SnapshotNative(bool, Option<String>, bool);

    impl Native for SnapshotNative {
        fn available(&self) -> bool {
            self.0
        }

        fn laid_out(&self) -> Result<(), String> {
            if self.2 {
                Err("the viewport is not laid out yet".into())
            } else {
                Ok(())
            }
        }

        fn bounds(&self, id: NodeId) -> Option<nsh_core::geometry::Bounds<Points>> {
            let (x, y, width, height) = match id {
                1 => (0.0, 0.0, 100.0, 100.0),
                2 => (80.0, 80.0, 30.0, 30.0),
                3 => (5.0, 5.0, 10.0, 10.0),
                _ => return None,
            };
            Some(Bounds::new(
                Point::new(Points(x), Points(y)),
                Size::new(Points(width), Points(height)),
            ))
        }

        fn viewport(&self) -> Size<Points> {
            Size::new(Points(50.0), Points(50.0))
        }

        fn press(&self, _: Point<Points>) {}
        fn long_press(&self, _: Point<Points>, _: std::time::Duration) {}
        fn scroll(&self, _: Point<Points>, _: Point<Points>) {}
        fn drag(&self, _: Point<Points>, _: Point<Points>) {}
        fn type_text(&self, _: &str) {}
        fn key(&self, _: nsh_testing::agent::Key) {}
        fn input_text(&self, id: NodeId) -> Option<String> {
            (id == 3).then(|| self.1.clone()).flatten()
        }
        fn back(&self) -> Result<(), String> {
            Ok(())
        }
        fn answer_dialog(&self, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn animating(&self) -> bool {
            false
        }
        fn screenshot(&self) -> Result<Vec<u8>, String> {
            Err("not available".into())
        }
        fn resize(&self, _: Points, _: Points) -> Result<(), String> {
            Ok(())
        }
        fn set_dark(&self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn quit(&self) {}
    }

    fn snapshot_host_with_availability(scene: SnapshotScene, available: bool) -> NshAgentHost {
        snapshot_host_with_native(scene, SnapshotNative(available, None, false))
    }

    fn snapshot_host_with_native(scene: SnapshotScene, native: SnapshotNative) -> NshAgentHost {
        let agent = Arc::new(Agent::requiring_native_hit_test(
            Arc::new(scene),
            Box::new(native),
        ));
        NshAgentHost::new("test", PlatformKind::Desktop, agent).unwrap()
    }

    fn snapshot_host(scene: SnapshotScene) -> NshAgentHost {
        snapshot_host_with_availability(scene, true)
    }

    fn sample_nodes() -> Vec<SemanticNode> {
        let mut root = SemanticNode::default();
        root.id = 1;
        root.role = "group".into();
        root.clipping = true;
        let mut child = SemanticNode::default();
        child.id = 2;
        child.parent = Some(1);
        child.test_id = Some("offscreen".into());
        child.hint = Some("reveal me".into());
        child.grouped = true;
        let mut visible = SemanticNode::default();
        visible.id = 3;
        visible.parent = Some(1);
        visible.test_id = Some("visible".into());
        visible.hint = Some("search gallery".into());
        visible.grouped = true;
        vec![root, child, visible]
    }

    #[test]
    fn worker_config_requires_loopback_and_nonempty_token() {
        assert!(validate_worker_config("127.0.0.1:17421".parse().unwrap(), "secret").is_ok());
        assert!(validate_worker_config("127.0.0.1:17421".parse().unwrap(), "  ").is_err());
        assert!(validate_worker_config("0.0.0.0:17421".parse().unwrap(), "secret").is_err());
    }

    #[test]
    fn snapshot_preserves_offscreen_metadata_and_clipped_visibility() {
        let host = snapshot_host(SnapshotScene(Ok(sample_nodes()), 1, Default::default()));
        let snapshot = host.snapshot();
        assert!(snapshot.ready);
        assert!(snapshot.nodes[0].visible);
        let child = &snapshot.nodes[0].children[0];
        assert_eq!(child.id, "offscreen");
        assert!(!child.visible);
        assert!(child.states.iter().any(|state| state == "hint: reveal me"));
        assert!(child.states.iter().any(|state| state == "grouped"));
        let compact = gpui_agent::compact_lines(&snapshot);
        assert!(compact.contains("hint: search gallery"));
        assert!(compact.contains("[grouped]"));
    }

    #[test]
    fn snapshot_builds_the_semantic_tree_once() {
        let builds = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let host = snapshot_host(SnapshotScene(Ok(sample_nodes()), 1, builds.clone()));
        let snapshot = host.snapshot();
        assert!(snapshot.ready);
        assert_eq!(snapshot.nodes[0].children.len(), 2);
        assert_eq!(builds.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn snapshot_never_shows_a_masked_input_text() {
        let secret = "hunter2";
        for masked in [true, false] {
            let mut nodes = sample_nodes();
            nodes[2].role = "textbox".into();
            nodes[2].input = true;
            if masked {
                nodes[2].states.push("masked".into());
                nodes[2].value = Some("\u{2022}".repeat(secret.len()));
            }
            let mut host = snapshot_host_with_native(
                SnapshotScene(Ok(nodes), 1, Default::default()),
                SnapshotNative(true, Some(secret.into()), false),
            );
            let snapshot = host.snapshot();
            let input = &snapshot.nodes[0].children[1];
            let lines = gpui_agent::compact_lines(&snapshot);
            if masked {
                assert_eq!(input.value.as_deref(), Some("•••••••"));
                assert!(input.states.iter().any(|state| state == "masked"));
                assert!(!lines.contains(secret), "{lines}");
                for op in [
                    Op::type_text("visible", secret),
                    Op::SetValue {
                        target: "visible".into(),
                        value: secret.into(),
                        expect_fingerprint: None,
                    },
                ] {
                    let reply = format!("{:?}", host.dispatch(&op));
                    assert!(!reply.contains(secret), "{reply}");
                }
            } else {
                assert_eq!(input.value.as_deref(), Some(secret));
            }
        }
    }

    #[test]
    fn scene_failure_is_not_a_healthy_empty_snapshot() {
        let host = snapshot_host(SnapshotScene(
            Err("model unavailable".into()),
            1,
            Default::default(),
        ));
        let snapshot = host.snapshot();
        assert!(!snapshot.ready);
        assert!(snapshot.nodes.is_empty());
    }

    #[test]
    fn snapshot_is_not_ready_before_the_first_batch() {
        let host = snapshot_host(SnapshotScene(Ok(sample_nodes()), 0, Default::default()));
        let snapshot = host.snapshot();
        assert!(!snapshot.ready);
        assert!(snapshot.nodes.is_empty());
    }

    #[test]
    fn snapshot_is_not_ready_while_the_viewport_is_not_laid_out() {
        let host = snapshot_host_with_native(
            SnapshotScene(Ok(sample_nodes()), 1, Default::default()),
            SnapshotNative(true, None, true),
        );
        let snapshot = host.snapshot();
        assert!(!snapshot.ready);
        assert!(snapshot.nodes.is_empty());
    }

    #[test]
    fn detached_surface_reports_unavailable() {
        let mut host = snapshot_host_with_availability(
            SnapshotScene(Ok(sample_nodes()), 1, Default::default()),
            false,
        );
        assert!(!host.hello().ready);
        assert!(!host.snapshot().ready);
        let error = host
            .dispatch(&Op::Invoke {
                name: "press".into(),
                args: serde_json::json!({"target":"visible"}),
            })
            .unwrap_err();
        assert!(error.contains("unavailable"));
    }

    #[test]
    fn semantic_ids_are_unique_even_when_test_ids_collide_with_fallbacks() {
        let mut identified = SemanticNode::default();
        identified.id = 1;
        identified.test_id = Some("n2".into());
        identified.role = "button".into();
        let mut fallback = SemanticNode::default();
        fallback.id = 2;
        let mut duplicate = SemanticNode::default();
        duplicate.id = 3;
        duplicate.test_id = Some("n2".into());
        let mut collision = SemanticNode::default();
        collision.id = 4;
        collision.test_id = Some("n2#2".into());
        let ids = stable_ids(&[identified, fallback, duplicate, collision]);
        assert_eq!(ids.get(&1).map(String::as_str), Some("n2#1"));
        assert_eq!(ids.get(&2).map(String::as_str), Some("n2#2"));
        assert_eq!(ids.get(&3).map(String::as_str), Some("n2#3"));
        assert_eq!(ids.get(&4).map(String::as_str), Some("n2#2#4"));
        assert_eq!(
            viewport_size(&serde_json::json!({"preset": "phone"})).unwrap(),
            (390.0, 844.0)
        );
        assert!(viewport_size(&serde_json::json!({"width": 0, "height": 10})).is_err());
        assert!(optional_point(&serde_json::json!({"x": 1})).is_err());
        assert_eq!(
            optional_point(&serde_json::json!({"x": 2, "y": 3})).unwrap(),
            Some(Point::new(Points(2.0), Points(3.0)))
        );
        assert!(number_arg(&serde_json::json!({"dx": "bad"}), "dx").is_err());
        assert!(number_arg(&serde_json::json!({"dx": 1e300}), "dx").is_err());
    }

    #[test]
    fn quit_menu_shortcuts_require_confirmation() {
        let mut binding = SemanticKeybinding::default();
        binding.id = "menu.File/Quit#1".into();
        binding.chord = "cmd+q".into();
        binding.label = Some("Quit".into());
        binding.menu_path = Some(vec!["File".into(), "Quit".into()]);
        let info = keybinding_info(&binding);
        assert!(gpui_agent::binding_is_dangerous(&info));
        binding.label = Some("Save".into());
        binding.chord = "ctrl+s".into();
        assert!(!keybinding_info(&binding).dangerous);
    }
}
