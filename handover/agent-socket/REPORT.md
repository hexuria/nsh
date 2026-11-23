# Final report: the agent work moved out of the fork into nsh-gpui-agent

Date: 2026-11-24. Orchestrated across sessions devin-7eb7d5a448484540b772f6556a2877a5 (orchestrator,
Linux+Android+web triage), devin-e0730bfce0f54d80a275b98a77bc54b6 (macOS+iOS),
devin-5171718ab21d47b785af639bda550158 (Windows).

## Deliverables

- The plug: https://github.com/hexuria/nsh-gpui-agent (public). `main` = the stage-0 move (22e8db6e).
  Platform crates on their own branches/PRs:
  - #2 agent bugfixes `devin/1795413363-agent-bugfixes` @0913a3f
    (compare: https://github.com/hexuria/nsh-gpui-agent/compare/main...devin/1795413363-agent-bugfixes)
  - #3 iOS `devin/1795407430-ios-stage1` @c4b3df7
  - #4 web `web` @8622da6
  - #5 linux `devin/linux-agent` @26bf494
  - #6 windows `devin/windows-agent` @cef7678
  - android `devin/1795407849-android` @3a35f66 (no PR opened)
  - macOS doors+adapter `devin/stage1-macos` @bd0d241
  - Integration pin used by the app: `devin/stage2-integration` @1e85cf65
- Fork branch `gpui-agent` on `hexuria/nsh` @56add34a0: upstream tag v0.6.1 (2f51d8611) + doors only.
- Fork branch `buwiz-next` on `hexuria/nsh` @04955773: `gpui-agent` + the 62 non-agent `buwiz` commits,
  unchanged.
- App branch `devin/stage3-plug-move` on `fullstackcrabs/buwiz-forms` @7443c2d3: plug dep @1e85cf65,
  `[patch]` to the sibling nsh checkout, `NSH_PINNED_REF` = buwiz-next 04955773. No PR opened — uriah's call.

## The socket

40 doors on `gpui-agent` (SOCKET.md at the branch root). Roughly 2,560 lines net:

| Group | Doors | Net lines |
|---|---|---|
| Foundation (nsh-testing) | 4 | ~260 |
| iOS | 4 | 179 |
| Linux | 4 | 171 |
| Web | 5 | 1,219 (includes tests + CLI module move) |
| Android | 2 (+ shared Model::frame 9) | 314 |
| Windows | 4 | 201 |
| macOS | 17 | 212 |

~2.6k lines of exposure against the 16,195 lines of agent work it replaces (~16%). Every door only
exposes; the logic stayed in the plug.

## Per platform: what ran, where, result

| Platform | Machine | Proof in the real eBIRForms app |
|---|---|---|
| macOS | e073's Mac | PROVEN: window plane :17421 real tree, ops verified; data plane :17422 bir-shell |
| Linux | this box (GTK4 docker + Xvfb) | PROVEN: same op set on the live window |
| Windows | 5171's box | PROVEN: 597-node tree, click→New taxpayer, TIN typed+read back, set-value, 92KB screenshot; data :17422 |
| iOS | e073 iPhone 17 sim (iOS 26.5) | PROVEN: 28→104 nodes, TIN typed+read back, plug screenshot 333KB; data :17422 |
| Android | this box emulator-5554 (buwiz_api35) | PROVEN: 28→104 nodes, TIN typed+read back, set-value, plug screenshot 96KB; data :17423 |
| Web | any | N/A for this app — its door deps are native-only (GTK4 print, SQLite, SFTP). Plug web crate proven at stage 1 vs examples/gallery |

App-side wiring that makes it bind (all local, uncommitted, on `devin/stage3-plug-move`):

1. Platform plug dep @1e85cf65 in `nsh/Cargo.toml` under the target cfg; `use nsh_gpui_agent_* as _;` links it.
2. `agent::install_window_plane()` at the top of `native()` in `nsh/src/lib.rs` — linking alone does nothing.
   (Android is the exception: the crate's `#[ctor]` installs itself when the cdylib loads.)
3. `choose()` in `nsh/src/agent.rs` requests `requested.port()+1` — the app's data plane binds
   `GPUI_AGENT_ADDR` at engine start before the surface exists, so the window plane keeps the base port.
4. The generated workspace `nsh/.nsh/rust/Cargo.toml` needs `[patch."https://github.com/hexuria/nsh"]`
   re-appended on every build (nsh regenerates it): without it the plug's git-dep `nsh-testing` resolves a
   second copy and `set_surface_hook`/`surface_opened` land in different OnceLocks. Local workaround is the
   `nsh/agent/cargo-with-agent.sh` CARGO wrapper; the durable fix belongs in nsh-cli's
   `write_workspace_manifest` (emit `[patch.{url}]` whenever any nsh-* dep resolves from a git source —
   scan the resolved graph, not just `Nsh::Git`).

Gates green on every platform: `just nsh-check && just nsh-build && just nsh-test` (137 Rust + full TSX suite).

## What stayed in the fork that is not a door (from SOCKET.md)

- macOS input provenance (event-number stamping, `synth_key` TLS scope, `keystroke_name`, text-field role
  mapping, `shown_part` clipping arithmetic): attribution/logic, lives in the plug.
- `AppView::ui_tree`/`act` as inherent methods: impossible outside the crate; plug uses `AppViewAgentExt`.
- `NshView` public required `ViewObj`/`Ivars` public (`ViewState` stays private).
- macOS startup wiring (shim `nsh-macos-host` registering door 4 before `nsh_macos_host::main()`): startup
  policy, in the plug.
- `Screen::long_pressable` pick change (Disabled vs None): logic; plug answers via door 1.
- buwiz's `Agent` changes: the plug's `Agent` carries them; `nsh test` runs upstream's untouched.
- `nsh-dev` SceneQuery additions (SemanticTree, Keybindings, Focused, HitAt, Focus, AppFocused): web doors
  3+5 give the plug page scripts and plain mounted data instead.
- `tray`/`global_menu` on `buwiz`: not agent work — they ride on `buwiz-next`.

## Rehearsal (Stage 3 of PLAN.md)

- 62 non-agent `buwiz` commits replayed onto `gpui-agent` tip: 29 clean / 33 conflict at 39 doors; ONE real
  door conflict (`abe8d514c`, iOS view.rs `Inner::load`); the rest missing skipped-commit context.
- `gpui-agent` rebased onto `longbridge/main` (958262185): **41 clean / 16 conflict** at 57 commits, all
  mechanical same-region overlap.
- `buwiz` onto the same main: **60 clean / 196 conflict** — the number the socket exists to beat.

## Findings for the owner to decide

1. **New doors wanted (4)**: key_focus setter (`key_focus::focus` is pub(crate)); window-aware
   `DevSurfaceView` (secondary-window hit tests/focus/resize — 4 findings); `Native::focused` impls (iOS +
   Windows); `Native::fire_keybinding` impls (semantic ids carry numeric keymap ids). Also a possible
   multi-rect `text_bounds` door (wrapped-link coverage).
2. **Recipe gaps (iOS + Android)**: `just nsh-run-agent` only delivers the data-plane token. iOS also needs
   `SIMCTL_CHILD_GPUI_AGENT`/`_ADDR` + `Library/Caches/gpui-agent-token` (0600); Android needs a
   post-launch relaunch carrying intent extras `GPUI_AGENT`/`GPUI_AGENT_ADDR` + `files/gpui-agent-token`
   (or invoke the plug's CLI launcher).
3. **Android nits**: data-plane token handoff is one-shot (any app restart needs it re-written); baked
   dev_env pre-shift + `choose()` +1 puts the data plane on 17423 while the recipe banner says 17422.
4. **Escalated review findings**: web DevTools port unauthenticated on loopback (real fix = pipe
   transport); duplicated-tab pairing takeover needs per-tab addressing; iOS focused-edit needs the
   focus door above.
5. **Quirks**: `app.info` reports `mode:"desktop"` on iOS and Android (data-plane door quirk, not the plug);
   `keybinding` ids are `keymap.<n>.<chord>` strings, not action names.
6. **Pending process**: plug PR merge order; repin stage-3 onto plug `main` once merged; whether to open
   the buwiz-forms PR for `devin/stage3-plug-move`; Stage 4 (gpui-agent 0.4.0, collapse the dual-versioned
   0.3.0/0.4.0 pin).
7. Review triage record: every Devin Review round on plug PRs #2–#6 is triaged in STATUS.md; all plug-side
   fixes pushed (latest rounds @0913a3f/@c4b3df7/@8622da6/@26bf494/@cef7678). The push token cannot
   comment on plug PRs — the commit + this ledger are the record.
