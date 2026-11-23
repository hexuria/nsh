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
| 2 | `nsh-testing` | `Screen::keymap(&self) -> Option<&Keymap>` (non-wasm) and `Screen::app_menu(&self) -> Option<&[MenuItem]>`, from `pub(crate)` to `pub` | The app's keyboard shortcuts and application menu as mounted. | all (keybindings, `fire_keybinding`) | +2 / -2 |
| 3 | `nsh-testing` | `Model::screen<R>(&self, read: impl FnOnce(&Screen) -> R) -> R` | Read access to the `Screen` a host's `Model` keeps. | all | +7 |
| 4 | `nsh-testing` | `set_surface_hook(hook: SurfaceHook) -> Result<(), SurfaceHook>`; `surface_opened(&OpenedSurface<'_>) -> Option<SurfaceAttachment>`; `type SurfaceHook = fn(&OpenedSurface<'_>) -> Option<SurfaceAttachment>`; `pub struct OpenedSurface<'a> { pub surface: &'a dyn Any }` with `new`; `pub struct SurfaceAttachment { pub model: Option<Arc<Model>>, pub guard: Box<dyn Any> }` with `new` | A hook a tool linked into a development host sets once; the host calls it when it opens a surface, keeps the returned model current (every batch and error, as for `nsh test`'s model) and drops the attachment when the surface closes. | every platform (where `buwiz` starts the agent today) | +74 (with its test) |
| iOS 1 | `nsh-platform-ios` | `dev::DevelopmentSurface`; `native(&self) -> Box<dyn Native>`; `with_views<R: Send>(&self, read: impl FnOnce(&UIView, &UIWindow, &MountedViews<'_>) -> R + Send) -> Option<R>`; `MountedViews::{is_available(&self) -> bool, get(&self, id: NodeId) -> Option<Retained<UIView>>}` | The existing native input driver and read access to the development surface's container, window and mounted views, on the main queue. Distinguishes an absent mounter from an unmounted node without retaining a mounter borrow across native calls. | iOS | +64 net |
| iOS 2 | `nsh-platform-ios` | `DevSession::start` calls `surface_opened(&OpenedSurface::new(&DevelopmentSurface))` and retains its `SurfaceAttachment` | Keeps the hook's model current independently of `nsh test`, resets it when the engine is replaced, and drops the attachment with the development session. | iOS | +61 |
| shared 5 | `nsh-testing` | `agent::Model::reset(&self)` | Clears the mounted scene and test setup when a host replaces its engine. | iOS (shared lifecycle) | +7 |
| iOS 3 | `nsh-platform-ios` | `DevelopmentSurface::on_connected(&self, callback: impl Fn(&DevClient, &dyn Fn(&str, &str)) + Send + Sync + 'static)` | The development client's connection event, its log channel, and the host's existing error overlay (`title`, `message`). The tool supplies all messages and chooses whether to show an error. | iOS | +38 net |
| shared 6 | `nsh-testing` | `agent::Model::frame(&self)` | Wakes waiters when the host receives a JavaScript animation frame. | Linux (shared lifecycle) | +9 |
| Linux 1 | `nsh-platform-linux` (`dev`) | `dev::DevSurface: Send + Clone`; `native(&self) -> Box<dyn Native>`; `on_main<R: Send + 'static>(&self, read: impl FnOnce(DevSurfaceView<'_>) -> R + Send + 'static) -> Option<R>`; view accessors `root() -> gtk4::Widget`, `window() -> Option<gtk4::Window>`, `widget(NodeId) -> Option<gtk4::Widget>` | The existing native driver and mounted widgets on the GTK main thread. Surface creation calls `surface_opened`; its attachment receives batches, errors and frames, resets on stop, and drops on surface close. Without a hook there is no attachment. | Linux | +114 / -1 |
| Linux 2 | `nsh-platform-linux` (`dev`) | `DevSurfaceView::node_at(&self, Point<Points>) -> Option<NodeId>`; `supports(&self, Keystroke) -> bool` | Existing native picking and read-only lookup of mounted shortcuts. | Linux | +33 |
| Linux 3 | `nsh-platform-linux` (`dev`) | `DevSurfaceView::menu(&self) -> Option<Arc<[MenuItem]>>`; `choose(&self, id: u32, role: Option<MenuRole>)` | The mounted application menu and its existing selection dispatch. Path walking stays in the plug. | Linux | +15 |

Foundation: 4 doors, about 260 lines in `nsh-testing`, against the 16,195 lines of agent work on `buwiz`.
iOS adds 4 doors, 170 net source lines across `nsh-testing` and `nsh-platform-ios`.
Linux adds 4 doors, +171 / -1 source lines across `nsh-testing` and `nsh-platform-linux`, reusing `Model::reset`.
The table lists 12 doors in total. Measure against release commit `2f51d8611` with `git diff --stat 2f51d8611`.

On this branch no crate depends on the `gpui-agent` library, no workspace member is named for it, and no file we
added has "agent" in its path.

## How the plug uses them

- **iOS launch.** The plug supplies a prebuilt host through the existing `NSH_RUNTIME` override; its `main` registers door 4 before calling `nsh_ios_host::main`. Its launcher reuses the shared `nsh_cli::main()` library entry added for web sessions, bootstraps the same development app ID to obtain its data container, writes a private one-use token there, and invokes `nsh run` with inherited `SIMCTL_CHILD_*` opt-in/address variables. No iOS-specific linking, token, or launch logic is added to the fork. Apps with their own Rust sets can call `ios::install()` during their existing generated-host registration instead.

- **The host's scene.** The plug's `nsh_agent_actions::SemanticScene` extends upstream's
  `nsh_testing::agent::Scene` with `semantic_tree`, `keybindings` and `long_press_disabled`, and is implemented for
  upstream's `Model` through doors 1 to 3. Upstream's `Scene` trait is unchanged.
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

- `buwiz`'s change to `Screen::long_pressable` (a long press on a disabled receiver is `Pick::Disabled`, not
  `Pick::None`) is logic, so it is not a door: the plug gets the same answer from door 1
  (`SemanticScene::long_press_disabled`).
- `buwiz`'s changes to upstream's ordinary agent (`Agent::run`, `long_press`, `settled`, strict checks inside
  `find`, the `TestStep::Synth` arm) are not on this branch: the plug's `Agent` has them; `nsh test` runs upstream's.
- The `SceneQuery` additions in `nsh-dev` (`SemanticTree`, `Keybindings`,
  `Focused`, `HitAt`, `Focus`, `AppFocused`) and `RemoteScene`'s semantic queries are not here yet: the platform
  that needs one adds it as a door in Stage 1 (Android calls `Model::frame`; web's `RemoteScene` needs the queries).
- `module.rs`/`run.rs` `tray` and `global_menu` on `buwiz` are not agent work: they ride on `buwiz-next`.

## Rows the platform sessions add

| Platform | Expected doors (from `handover/agent-socket/data/internal-names-by-platform.txt`) |
|---|---|
| macOS | the surface handle for door 4; `view::superview`, `provenance::*`, `input::{later, view_at, event, enter, focus_control, root_in}`, `AppView::act`, `synth`, `scroll::reveal`, `window::root_in`, `keymap::bindings`, `text_input::*`, `MacNative::activate` |
| Windows | the surface handle; `input::pick_node`, `text_input::{focus, focused}`, `chrome::fire_menu_keybinding` |
| Android | the surface handle; `Model::frame`; a way for the plug to supply `TestAgent.java`'s additions |
| iOS | the surface handle from `DevSession` |
| Web | the CLI's web listener (`nsh-cli/src/web_agent.rs`); `SceneQuery` additions for `RemoteScene` |
