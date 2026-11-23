# SOCKET.md: the doors on branch `gpui-agent`

Branch `gpui-agent` of `hexuria/nsh` is upstream `longbridge/nsh` tag `v0.6.1` (`2f51d8611`) plus the doors below,
and nothing else. A door only exposes something that is already in NSH (an accessor, a read-only view, a hook that
calls out). It holds no gpui-agent logic and does not name gpui-agent. All behavior lives in
[`hexuria/nsh-gpui-agent`](https://github.com/hexuria/nsh-gpui-agent) (the plug).

This file is the contract the platform sessions work to, and the list the upstream handover turns into PRs.
Each door is one commit, `<area>: <What it does>`.

## Doors

| # | Crate | Item and signature | What it exposes | Needed by | Lines |
|---|---|---|---|---|---|
| 1 | `nsh-testing` | `Screen::mounted(&self, id: NodeId) -> Option<MountedView<'_>>`; `Screen::children(&self, id: NodeId) -> &[NodeId]`; `Screen::stack_screens(&self, id: NodeId) -> Vec<NodeId>`; `pub struct MountedView<'a>` (read-only getters: `kind`, `parent`, `events`, `test_id`, `label`, `text`, `frame`, `input_text`, `secure`, `scrolls`, `clips`, `hidden`, `disabled`, `selected`, `role`, `accessibility_state`, `accessibility_hint`, `accessibility_value`, `accessible`, `accessibility_hidden`, `canvas_node`, `span`); `pub struct MountedSpan<'a> { view, text, start, end }` | The views the screen mirrored from the applied batches, with the props they were mounted with. The screen also keeps four accessibility props it received and dropped before (`hint`, `value`, `accessible`, `hidden`). | all (the plug's semantic tree) | +178 / -1 |
| 2 | `nsh-testing` | `Screen::keymap(&self) -> Option<&Keymap>` and `Screen::app_menu(&self) -> Option<&[MenuItem]>`, from `pub(crate)` to `pub` | The app's keyboard shortcuts and application menu as mounted. Web 5 also makes `keymap` available on wasm. | all (keybindings, `fire_keybinding`) | +2 / -2 |
| 3 | `nsh-testing` | `Model::screen<R>(&self, read: impl FnOnce(&Screen) -> R) -> R` | Read access to the `Screen` a host's `Model` keeps. | all | +7 |
| 4 | `nsh-testing` | `set_surface_hook(hook: SurfaceHook) -> Result<(), SurfaceHook>`; `surface_opened(&OpenedSurface<'_>) -> Option<SurfaceAttachment>`; `type SurfaceHook = fn(&OpenedSurface<'_>) -> Option<SurfaceAttachment>`; `pub struct OpenedSurface<'a> { pub surface: &'a dyn Any }` with `new`; `pub struct SurfaceAttachment { pub model: Option<Arc<Model>>, pub guard: Box<dyn Any> }` with `new` | A hook a tool linked into a development host sets once; the host calls it when it opens a surface, keeps the returned model current (every batch and error, as for `nsh test`'s model) and drops the attachment when the surface closes. | every platform (where `buwiz` starts the agent today) | +74 (with its test) |
| iOS 1 | `nsh-platform-ios` | `dev::DevelopmentSurface`; `native(&self) -> Box<dyn Native>`; `with_views<R: Send>(&self, read: impl FnOnce(&UIView, &UIWindow, &MountedViews<'_>) -> R + Send) -> Option<R>`; `MountedViews::{is_available(&self) -> bool, get(&self, id: NodeId) -> Option<Retained<UIView>>}` | The existing native input driver and read access to the development surface's container, window and mounted views, on the main queue. Distinguishes an absent mounter from an unmounted node without retaining a mounter borrow across native calls. | iOS | +64 net |
| iOS 2 | `nsh-platform-ios` | `DevSession::start` calls `surface_opened(&OpenedSurface::new(&DevelopmentSurface))` and retains its `SurfaceAttachment` | Keeps the hook's model current independently of `nsh test`, delivers `Model::engine_started` with each new engine, resets it when the engine is replaced, and drops the attachment with the development session. | iOS | +70 |
| shared 5 | `nsh-testing` | `agent::Model::reset(&self)` | Clears the mounted scene and test setup when a host replaces its engine. | iOS (shared lifecycle) | +7 |
| iOS 3 | `nsh-platform-ios` | `DevelopmentSurface::on_connected(&self, callback: impl Fn(&DevClient, &dyn Fn(&str, &str)) + Send + Sync + 'static)` | The development client's connection event, its log channel, and the host's existing error overlay (`title`, `message`). The tool supplies all messages and chooses whether to show an error. | iOS | +38 net |
| shared 6 | `nsh-testing` | `agent::Model::frame(&self)` | Wakes waiters when the host receives a JavaScript animation frame. | Linux (shared lifecycle) | +9 |
| Linux 1 | `nsh-platform-linux` (`dev`) | `dev::DevSurface: Send + Clone`; `native(&self) -> Box<dyn Native>`; `on_main<R: Send + 'static>(&self, read: impl FnOnce(DevSurfaceView<'_>) -> R + Send + 'static) -> Option<R>`; view accessors `root() -> gtk4::Widget`, `window() -> Option<gtk4::Window>`, `widget(NodeId) -> Option<gtk4::Widget>` | The existing native driver and mounted widgets on the GTK main thread. Surface creation calls `surface_opened`; its attachment receives batches, errors and frames, resets on stop, and drops on surface close. Without a hook there is no attachment. | Linux | +114 / -1 |
| Linux 2 | `nsh-platform-linux` (`dev`) | `DevSurfaceView::node_at(&self, Point<Points>) -> Option<NodeId>`; `supports(&self, Keystroke) -> bool` | Existing native picking and read-only lookup of mounted shortcuts. | Linux | +33 |
| Linux 3 | `nsh-platform-linux` (`dev`) | `DevSurfaceView::menu(&self) -> Option<Arc<[MenuItem]>>`; `choose(&self, id: u32, role: Option<MenuRole>)` | The mounted application menu and its existing selection dispatch. Path walking stays in the plug. | Linux | +15 |
| Web 1 | `nsh-cli` | Library target `nsh_cli`; `main() -> ExitCode`; `set_web_session_hook(WebSessionHook) -> Result<(), WebSessionHook>`; `type WebSessionHook = fn(&WebSession<'_>) -> anyhow::Result<Option<Box<dyn Any>>>`; `WebSession<'a> { server: Arc<DevServer>, entry: &'a Path, name: &'a str, rust_root: bool }`; re-exported `note`, `stop_requested`, `Stoppable` | A binary linked to the CLI can attach to a started Web development session. The CLI retains the returned attachment until watch ends and propagates hook errors. No hook leaves ordinary runs unchanged. | Web (library entry also reused by launchers) | +132 net (includes the main-to-library move and tests) |
| Web 2 | `nsh-dev`, `nsh-cli` | `DevServer::add_page_script(&self, source: String)` | Registers scripts served and loaded, in order, before the development page boots. No registered scripts leaves the page unchanged. | Web | +77 / -1 |
| Web 3 | `nsh-dev`, `nsh-platform-web`, `nsh-testing`, `nsh-cli` | `TestStep::Page { name: String, args: serde_json::Value }`; `globalThis.nshPageSteps[name](args, player)` | Dispatches named steps to page-supplied handlers through the existing step-answer path, including asynchronous results and failures. Unknown names fail. Works for ordinary tests and observers. | Web | +84 / -6 |
| Web 4 | `nsh-dev`, `nsh-platform-web`, `nsh-cli` | `DevServer::set_observer_hook(&self, fn(SocketAddr, &serde_json::Value) -> bool)`; `ClientMessage::Hello.extra: Option<Value>` from `globalThis.nshHelloExtra`; `ServerMessage::Hello.observer: bool`; `player.observeStart()` / `nsh_observe_start()` | Lets a tool select step-capable hosts as observers without sending test setup. Their model tracks the running app and resets on engine replacement; storage, network and clock stay unchanged. Selection policy stays in the tool. | Web | +195 / -10 |
| Web 5 | `nsh-testing`, `nsh-dev` | `snapshot::ScreenSnapshot` and nested owned serde DTOs; `Screen::snapshot(&self) -> ScreenSnapshot`; `SceneQuery::ScreenSnapshot`; `Scene::screen_snapshot(&self) -> Result<ScreenSnapshot, String>` implemented by `RemoteScene`; `Screen::keymap` on wasm | Mounted view props, hierarchy, stack screens, keymap and an icon-free menu as plain data, answered from the host's model. Semantic interpretation and secure-text masking stay in the plug. | Web (DTOs also usable by other hosts) | +751 / -3 |
| Android 1 | `nsh-platform-android` | `#[non_exhaustive] pub struct AndroidSurface`; `native(&self) -> Box<dyn Native>`; `density(&self) -> f32`; `view(&self) -> &GlobalRef`; `java_vm(&self) -> *mut jni::sys::JavaVM`; `Native.connectDevServer` calls `surface_opened` and keeps its `SurfaceAttachment` | The existing native input driver, display density, `NshView` global reference and Java VM of the development surface, read-only. The host feeds the attachment's model every batch, error and frame, resets it when the engine is replaced, and drops the attachment in `Native.destroy`. | Android | +107 |
| shared 7 | `nsh-cli` | `#[non_exhaustive] pub struct DevelopmentHostLaunch<'a>`; `device(&self) -> &str`; `set_environment(&mut self, name: &str, value: &str)`; `write_private_file(&self, path: &str, contents: &[u8]) -> anyhow::Result<()>`; `set_development_host_launch_hook(hook: DevelopmentHostLaunchHook) -> Result<(), DevelopmentHostLaunchHook>`; `type DevelopmentHostLaunchHook = fn(&mut DevelopmentHostLaunch<'_) -> anyhow::Result<Option<Box<dyn Any>>>` | The lifecycle point after a development host is installed and before it starts. Its Android backend reports the adb serial, maps environment-like values to start-intent strings, and writes a private host file over adb standard input (never in arguments). The returned attachment is kept until the run ends. | Android | +198 |
| macOS 1 | `nsh-platform-macos` (`dev`) | `pub struct SurfaceHandle(u64)` (`Copy`, `key(self) -> u64`); `Surface::new` calls `surface_opened(&OpenedSurface::new(&handle))` and retains the `SurfaceAttachment` | A key-based handle for the surface a development host opened. The attachment's model is fed on batches, errors and frames, reset when the engine is replaced, and dropped with the surface. | macOS | +60 / -2 |
| macOS 2 | `nsh-platform-macos` | `pub struct MacNative`; `MacNative::new(key: u64) -> Self` | The platform's ordinary native input driver, for the plug to wrap rather than copy. | macOS | +4 / -3 |
| macOS 3 | `nsh-platform-macos` | `Surface::key(&self) -> u64`; `Surface::with_key(key: u64) -> Option<Rc<Surface>>`; `Surface::root(&self) -> NshView` | Surface identity, key lookup and the mounted root view. | macOS | +8 / -3 |
| macOS 4 | `nsh-platform-macos` | `text_input::InputState::masked_len(&self) -> Option<usize>`; `text_input::InputState::plain_text(&self) -> String`; `text_input::pad_owns(input: &InputState, hit: &NSView) -> bool` | Read-only input text state and ownership of a lifted input pad. | macOS | +23 / -2 |
| macOS 5 | `nsh-platform-macos` | `view::superview(view: &NSView) -> Option<Retained<NSView>>`; `pub struct NshView` with `from_ns`, `ns`, `id`, `kind`, `parent_view`, `child_views`, `test_id`, `events`, `accessibility`, `text`, `style_hidden`, `clips_children`, `input` and `native_views` | Read-only access to mounted views and their props through `NshView` methods, without exposing `ViewState`. | macOS | +56 / -12 |
| macOS 6 | `nsh-platform-macos` | `pub enum access::Ax`; `pub enum access::Value`; `pub struct access::Spec` (public fields); `access::spec_for(...) -> Spec` | The platform's accessibility role/value mapping as data. | macOS | +58 / -16 |
| macOS 7 | `nsh-platform-macos` | `keymap::bindings(surface: &Surface) -> Vec<KeyBinding>` | The surface's mounted key bindings. | macOS | +11 / -1 |
| macOS 8 | `nsh-platform-macos` | `input::{enter, root_in, event, focus_control, later}`; `input::view_at(root: &NshView, x: f32, y: f32) -> Option<NshView>` | Existing input routing and scheduling operations plus read-only hit resolution. | macOS | +12 / -6 |
| macOS 9 | `nsh-platform-macos` | `window::root_in(surface: &Surface, window: &NSWindow) -> Option<NshView>` | The root view mounted in an app-owned window. | macOS | +2 / -2 |
| macOS 10 | `nsh-platform-macos` | `scroll::bounds_origin(view: &NshView) -> (f32, f32)`; `scroll::reveal(surface: &Rc<Surface>, target: &NshView)` | Existing scrolling geometry and reveal behavior. | macOS | +3 / -3 |
| macOS 11 | `nsh-platform-macos` | `text_input::{focus(view: &NshView), editing_in(root: &NshView) -> bool}`; `text_input::test_focused(window: &NSWindow) -> Option<NodeId>` under `feature = "dev"` | Existing input focus operations and focus queries. | macOS | +3 / -3 |
| macOS 12 | `nsh-platform-macos` (`dev`) | `synth::{to_window, mouse, mouse_event, timestamp, wheel_with, type_text, key, shot}` | The existing synthesized input and screenshot primitives, without provenance marking. | macOS | +12 / -9 |
| macOS 13 | `nsh-platform-macos` (`dev`) | `AppView::surface_handle(&self) -> SurfaceHandle` | The key handle of the surface an `AppView` runs. | macOS | +6 |
| macOS 14 | `nsh-platform-macos` | `Surface::wants_frames(&self) -> bool`; `Surface::input_pending(&self) -> bool` under `feature = "dev"` | Whether frame/animation work is in flight and whether an engine batch is waiting for the main thread. The plug keeps the batch count and settle policy. | macOS | +7 / -1 |
| macOS 15 | `nsh-platform-macos` | `pub mod file_drop`; `file_drop::drop_uris(surface: &Surface, x: f32, y: f32, uris: Vec<String>) -> bool`; `input::event_root(surface: &Surface) -> NshView` | The existing file-drop dispatch and the root of the window the current input belongs to, so a caller can preserve pointer routing around it. | macOS | +3 / -3 |
| macOS 16 | `nsh-platform-macos` | `NshView::surface(&self) -> Option<Rc<Surface>>` | The surface owning a view, read by walking ancestor roots. This replaces the routing-based lookup with a pure read, a behavior-neutral correction within the visibility door. | macOS | +11 / -1 |
| macOS 17 | `nsh-platform-macos` | `window::root_view(surface: &Surface, id: NodeId) -> Option<NshView>`; `window::key_window(surface: &Surface) -> Option<Retained<NSWindow>>` under `feature = "dev"`; `window::window_of(surface: &Surface, id: NodeId) -> Option<Retained<NSWindow>>`; `window::roots(surface: &Surface) -> Vec<NshView>` | The surface's logical app-window registry: roots, the window showing one, and the key window. | macOS | +4 / -4 |

Foundation: 4 doors, about 260 lines in `nsh-testing`, against the 16,195 lines of agent work on `buwiz`.
iOS adds 4 doors, 179 net source lines across `nsh-testing` and `nsh-platform-ios`.
Linux adds 4 doors, +171 / -1 source lines across `nsh-testing` and `nsh-platform-linux`, reusing `Model::reset`.
Web adds 5 doors, 1,219 net lines including tests and the CLI module move (individual commit counts above).
Android adds 2 doors, 305 source lines across `nsh-platform-android` and `nsh-cli`; its shared `Model::frame` door adds 9 lines in `nsh-testing`.
macOS adds 17 doors, +283 / -71 lines in `nsh-platform-macos` (212 net), reusing the shared surface hook and lifecycle rows.
The table lists 36 doors in total. Measure against release commit `2f51d8611` with `git diff --stat 2f51d8611`.

On this branch no crate depends on the `gpui-agent` library, no workspace member is named for it, and no file we
added has "agent" in its path.

## How the plug uses them

- **iOS launch.** The plug supplies a prebuilt host through the existing `NSH_RUNTIME` override; its `main` registers door 4 before calling `nsh_ios_host::main`. Its launcher reuses the shared `nsh_cli::main()` library entry added for web sessions, bootstraps the same development app ID to obtain its data container, writes a private one-use token there, and invokes `nsh run` with inherited `SIMCTL_CHILD_*` opt-in/address variables. No iOS-specific linking, token, or launch logic is added to the fork. Apps with their own Rust sets can call `ios::install()` during their existing generated-host registration instead.

- **Android launch.** The plug uses shared 7 rather than copying the iOS launcher pattern: Android has no `SIMCTL_CHILD`-style environment inheritance into another app's process, the host cannot accept its token before the package exists, and the stock command line keeps APK assembly, signing, installation and the immediately following `am start` inside one private sequence. A launcher outside the library cannot add the start intent's values or know when that sequence reaches its one safe write point without duplicating the packaging code or racing the launch. The door itself is platform-neutral; only the backend needed here is Android's.

- **The host's scene.** The plug's `nsh_agent_actions::SemanticScene` extends upstream's
  `nsh_testing::agent::Scene` with `semantic_tree`, `keybindings` and `long_press_disabled`, and is implemented for
  upstream's `Model` through doors 1 to 3. Web 5 supplies the plain snapshot reader for `RemoteScene`.
- **The host's native driver.** The plug's `nsh_agent_actions::StrictNative` extends upstream's
  `nsh_testing::agent::Native` with the ten methods `buwiz` had added to `Native`: `available`, `hit_at`,
  `validate_keys`, `focus_node`, `fire_menu_keybinding`, `app_focused`, `rotate`, `keyboard`, `laid_out`,
  `take_failure` (same defaults). Upstream's `Native` trait is unchanged.
- **The agent actions.** The 38 functions `buwiz` added to `nsh_testing::agent::Agent` are the plug's own
  `nsh_agent_actions::Agent`, on top of `Arc<dyn SemanticScene>` and `Box<dyn StrictNative>`, with the private
  helpers they need (settling, reveal-scrolling, landing, strict actionability) copied from `buwiz`. Upstream's
  `Agent` (the ordinary `nsh test --target` agent) is upstream's `v0.6.1` code, untouched.
- **A place to start.** Door 4. The plug's platform part registers a `SurfaceHook`; each platform's development host
  calls `surface_opened` at the spot where `buwiz`'s `dev-agent` feature called `nsh_gpui_agent::start_from_env`.

## What stayed out of the branch, and why

- macOS input provenance stays in the plug. Event-number stamping and the `synth_key` thread-local scope are attribution logic, not visibility: the plug constructs the same mouse events with buwiz's stamp and wraps the exposed key/type synthesis entry points. Key-name synthesis (`keystroke_name`), mapping inputs to AppKit's text-field role, and buwiz's `shown_part` clipping arithmetic are copied logic too; `shown_part` is computed externally from the exposed parent, clipping, frame and scroll geometry.
- `AppView::ui_tree` and `AppView::act` cannot be inherent methods outside their crate. The plug supplies them through its extension trait `AppViewAgentExt`; the fork exposes only the surface, view, input and window state they read.
- macOS startup wiring stays in the plug: its `nsh-macos-host` shim binary registers door 4 before delegating to `nsh_macos_host::main()`, and `NSH_RUNTIME` supplies that binary to `nsh run --target macos`. Because the visibility/synthesis doors are dev-gated, a consumer selecting only the in-process server still receives `nsh-platform-macos`'s `dev` feature through feature unification; that build-time difference is accepted rather than moving startup policy into the fork.
- Making `NshView` public requires `ViewObj` and its `Ivars` type to be public as well: Rust otherwise rejects the public `Deref` target as a private type in a public interface. `ViewState` remains private.
- `NshView::surface()` now reads the owning surface from ancestor roots without calling `input::enter` or otherwise routing input. That is a behavior-neutral correction within what is itself a visibility door, not agent logic.
- `buwiz`'s change to `Screen::long_pressable` (a long press on a disabled receiver is `Pick::Disabled`, not
  `Pick::None`) is logic, so it is not a door: the plug gets the same answer from door 1
  (`SemanticScene::long_press_disabled`).
- `buwiz`'s changes to upstream's ordinary agent (`Agent::run`, `long_press`, `settled`, strict checks inside
  `find`, the `TestStep::Synth` arm) are not on this branch: the plug's `Agent` has them; `nsh test` runs upstream's.
- The tool-specific `SceneQuery` additions in `nsh-dev` (`SemanticTree`, `Keybindings`,
  `Focused`, `HitAt`, `Focus`, `AppFocused`) remain out. Web 3 lets page scripts answer DOM questions;
  Web 5 supplies plain mounted data for the plug to build semantic trees and keybindings.
- `module.rs`/`run.rs` `tray` and `global_menu` on `buwiz` are not agent work: they ride on `buwiz-next`.

## Rows the platform sessions add

| Platform | Expected doors (from `handover/agent-socket/data/internal-names-by-platform.txt`) |
|---|---|
| macOS | recorded above as macOS 1–17 |
| Windows | the surface handle; `input::pick_node`, `text_input::{focus, focused}`, `chrome::fire_menu_keybinding` |
| Android | the surface handle; `Model::frame`; a way for the plug to supply `TestAgent.java`'s additions |
| iOS | the surface handle from `DevSession` |
| Web | recorded above as Web 1–5: CLI attachment, page scripts and steps, observer hosts and owned screen snapshots |
