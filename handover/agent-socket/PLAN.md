# Plan: move our gpui-agent work out of the NSH fork

Branch `feat/nsh-agent-socket` of `hexuria/nsh`. This branch carries the handover only; it is not merged anywhere.
The companion handover, for sending our other changes upstream, is branch `feat/nsh-upstream-prs`.

Everything under "Measured" was measured on 2026-11-23 on `hexuria/nsh` branch `buwiz` at `70d8ff26b`, against
upstream tag `v0.6.1` (`2f51d8611`). The data files in `data/` hold the full lists. Re-measure before relying on a
number: upstream moves about 95 commits a day.

## The problem

`hexuria/nsh` branch `buwiz` is upstream NSH 0.6.1 plus our own work. Most of that work makes an NSH app drivable by
[gpui-agent](https://github.com/hexuria/gpui-agent). Upstream will not take it: our PR
[longbridge/nsh#449](https://github.com/longbridge/nsh/pull/449) was closed with "not ready to accept this feature",
and upstream's `CONTRIBUTING.md` says what one app needs belongs in the app's own crate or in an nshm, not in NSH.

We wrote it inside upstream's files, so every upstream change to those files collides with ours.

## The decision (owner's picks, final)

| Question | Pick |
|---|---|
| Road | **Thin socket.** The fork keeps only the few public doors we need. All the logic moves to crates we own. |
| Carry the socket as | A short branch of small commits that we rebase onto each upstream release. |
| Offer doors upstream | Yes, as small generic PRs. The upstream handover (`feat/nsh-upstream-prs`) does this, from the door list this plan produces. |
| The plug lives in | A new repository of ours: `hexuria/nsh-gpui-agent` (created by the owner, empty). |
| What moves out | The adapter crate, the agent actions, macOS window control, and the other platforms (Linux, Windows, Android, iOS, web). |
| First step | All platforms at once. The owner runs Devin sessions on Linux, macOS and Windows machines, with the Android emulator and the iOS Simulator, so each platform is built and run for real. |
| Catch up with upstream | At each NSH release. |

The non-agent changes (menus, tray, Windows and macOS fixes) are **not** part of this plan. They stay on the fork line
untouched; the upstream handover deals with them.

## Words used here

- **Socket**: the changes that stay in the fork. Each one only *exposes* something (makes an item public, adds an
  accessor, adds a hook that calls out). A socket change contains no gpui-agent logic and does not name gpui-agent.
- **Door**: one socket change.
- **Plug**: our crates in the new repository. All behavior lives here.

## Measured

What we carry on top of upstream `v0.6.1`:

| | |
|---|---|
| Our commits (non-merge) | 242 |
| Files changed | 211 |
| Lines added / removed | 23,013 / 1,558 |
| New files of ours | 26 files, 9,315 lines |
| Edits inside upstream's files | 185 files, 13,698 added, 1,558 removed |
| Commits that are agent work (subject says so, or they touch a file with "agent" in its path) | 156 commits, 16,195 lines |
| Commits that are other work | 86 commits, 5,963 lines |

The agent files (`data/agent-files.tsv`):

| File | Ours or theirs | Lines we added |
|---|---|---|
| `crates/nsh-testing/src/agent.rs` | theirs, edited | 3,220 (file: 3,256 at v0.6.1, 6,225 on buwiz; upstream main has since grown it too) |
| `crates/nsh-platform-macos/src/gpui_agent.rs` | ours, new | 2,777 |
| `crates/nsh-gpui-agent/src/lib.rs` | ours, new crate | 1,436 |
| `crates/nsh-platform-macos/tests/native/*agent*` | ours, new | 700 |
| `crates/nsh-platform-windows/src/agent.rs` | theirs, edited | 287 |
| `crates/nsh-platform-android/src/agent.rs` and `java/.../TestAgent.java` | theirs, edited | 284 and 220 |
| `crates/nsh-cli/src/web_agent.rs` | ours, new | 198 |
| `crates/nsh-platform-linux/src/agent.rs` | theirs, edited | 160 |
| `crates/nsh-platform-macos/src/agent.rs` | theirs, edited | 136 |
| `crates/nsh-platform-ios/src/agent.rs` | theirs, edited | 58 |
| gallery fixtures and tests `examples/gallery/tests/*agent*` | ours, new | 149 |

Why the socket can be small:

- **The adapter is already a wrapper.** `nsh-gpui-agent` holds an `Arc<nsh_testing::agent::Agent>` and implements
  gpui-agent's `AgentHost` on it. It depends only on `gpui-agent`, `nsh-core` and `nsh-testing`.
- **What it stands on is the part inside NSH.** It calls 24 `Agent` methods; upstream `v0.6.1` has none of them. In
  `nsh-testing/src/agent.rs` we added 38 public functions, 2 methods to the `Scene` trait and 10 to the `Native`
  trait (`data/testing-agent-api-we-added.txt`).
- **The platform code touches few internals.** The 2,777-line macOS file uses 18 distinct `crate::` names, 26
  references in all. Our edits to the Linux and Windows `agent.rs` use 4 each
  (`data/internal-names-by-platform.txt`).
- **Upstream already has public seams here.** `nsh_testing::agent` exports `trait Scene`, `trait Native`, `struct
  Model`, `Agent::new(scene, native)` and `Agent::take(step)`.

Not measured: the socket's real size. This plan's first stage measures it.

Worth knowing: since `v0.6.1`, upstream main added methods of its own to those traits (`Scene`: `accessibility`,
`layout`, `window_ids`, `window_size`, `set_window_size`; `Native`: `covered`, `opening`, `scroll_insets`, `try_key`,
`try_type_text`). Some may do what ours do. Not checked one for one.

## Target shape

```
longbridge/nsh  vX.Y.Z  (an upstream release tag)
   └─ hexuria/nsh  branch gpui-agent        <- the NSH that nsh-gpui-agent is built for
        = that tag
        + socket commits (doors only; one small commit each)
        + nothing else
        └─ hexuria/nsh  branch buwiz-next    <- the NSH the app pins
             = branch gpui-agent
             + our non-agent commits (unchanged; the upstream handover shrinks these)

hexuria/nsh-gpui-agent  (the plug; not a fork of anything)
   ├─ the adapter            (today: crates/nsh-gpui-agent)
   ├─ the agent actions      (today: our additions to nsh-testing/src/agent.rs)
   └─ one part per platform  (today: gpui_agent.rs, and our edits to each platform's agent.rs)

hexuria/buwiz-forms  nsh/
   depends on the plug, and on NSH at the buwiz-next pin
```

Two branches, because they answer two different questions. `gpui-agent` is "which NSH does the plug need": an
upstream release plus the doors, and nothing that is only the app's. `buwiz-next` is "which NSH does the app
build": the same, plus the app's other changes. Keeping them apart means the plug's README can name one exact NSH,
and the doors can be rebased and offered upstream without the app's menu and tray work in the way.

After the move, on `gpui-agent`: no crate depends on the `gpui-agent` library, no workspace member is named for
it, and no file we added has "agent" in its path. What is left of our agent work there is the socket.

**One NSH per build.** The app takes every `nsh-*` crate from one checkout, by path (`nsh/Cargo.toml` in
`hexuria/buwiz-forms` says so). Cargo treats the same crate from two sources as two different crates, so if the
plug pulled NSH from Git while the app used its checkout, their `Agent` and `Scene` types would not match. The plug
therefore declares NSH as a Git dependency on branch `gpui-agent`, and an app that uses a different NSH line
redirects that dependency to its own checkout with a `[patch]` section. That only works because `buwiz-next`
contains everything `gpui-agent` has.

## Decisions made to fill gaps

The owner did not pick these. Change any of them before starting; do not re-ask them mid-run.

1. **Repository:** `hexuria/nsh-gpui-agent` exists, public and empty. Its license is Apache-2.0, as NSH's and
   gpui-agent's are. Its `README.md` must say which NSH to use; see "The plug's README" below.
2. **The plug is Rust crates an app links**, usable from the app's own crate. Shape it so it can also be installed
   as an nshm (`nsh add`) once the fork is on an NSH that has nshm; `buwiz` at 0.6.1 does not have it yet.
3. **The fork gets two new branches.** `gpui-agent` is an upstream release plus the doors only. `buwiz-next` is
   `gpui-agent` plus our non-agent commits, and is what the app pins. `buwiz` is not rewritten or force-pushed. The
   app moves by changing its pin, in a PR the owner merges; renaming `buwiz-next` to `buwiz` afterwards is the
   owner's call.
4. **gpui-agent stays at `=0.3.0` during the move**, so behavior is compared like for like. Moving the plug to 0.4.0
   is a separate last step.
5. **What cannot move stays in the socket, with a written reason.** Likely candidates: `nsh-cli/src/web_agent.rs`
   (the CLI hosts the web listener) and `TestAgent.java` (it is compiled into the Android host). Try first; a door
   that lets the plug supply them is better than leaving them in.
6. **A door is generic.** It is named for what it exposes ("the host's scene and native driver", "the view under a
   point"), it has a doc comment that makes sense without gpui-agent, and it would be reasonable to offer upstream.
7. **Behavior does not change.** This is a move. A bug found on the way is written down and fixed in a separate
   commit after the move is proven.

## Stages

### Stage 0: foundations (one session, before the others start)

1. Read, in this order: the fork's `AGENTS.md`; `docs/dev/tooling.md` (the "GPUI-agent development" sections for
   macOS, Android, iOS and web); `GPUI_AGENT_REPORT.md`; upstream's `CONTRIBUTING.md` section "Scope: what NSH
   accepts"; this plan's `data/` files.
2. Start branch `gpui-agent` on `hexuria/nsh` from upstream tag `v0.6.1`. In `hexuria/nsh-gpui-agent`, set up the
   workspace, move `crates/nsh-gpui-agent` into it unchanged, and make it build against branch `gpui-agent` by Git
   dependency. Write the README (below).
3. Design the socket for `nsh-testing`. The plug needs two things from a running host: the `Scene` and `Native` it
   already built, and a place to start from when the host is ready (today each platform's `dev-agent` feature calls
   `nsh_gpui_agent::start_from_env`). Decide how the plug gets them through doors, and how the 38 functions and 12
   trait methods we added become the plug's own code on top of upstream's public `Scene` and `Native`.
4. Write `SOCKET.md` at the root of branch `gpui-agent`: one row per door, with the crate, the item and its
   signature, what it exposes, which platform needs it, and the lines it adds. This file is the contract the
   platform sessions work to, and the list the upstream handover turns into PRs.
5. Move the agent actions out of `nsh-testing/src/agent.rs` into the plug. That file ends as upstream's plus doors.

Stage 0 is done when the adapter and the agent actions build in `hexuria/nsh-gpui-agent` against branch
`gpui-agent`, the headless tests that covered them pass there, `SOCKET.md` exists, and the README names the NSH to
use.

#### The plug's README

`hexuria/nsh-gpui-agent` has a starting `README.md`. Keep it true as the work goes on. It must always say:

- **Which NSH this builds against:** `https://github.com/hexuria/nsh`, branch `gpui-agent`, and the exact commit the
  plug is tested with.
- **What that branch is:** an upstream `longbridge/nsh` release (name the tag) plus the doors in its `SOCKET.md`,
  and nothing else. Upstream NSH on its own does not work, and why (the doors are not upstream yet).
- **How an app uses it:** the dependency lines, and the `[patch]` an app needs when it builds NSH from its own
  checkout, because a build can only have one NSH.
- **Which gpui-agent version** it speaks.
- **What works on which platform**, from the Stage 1 results.

### Stage 1: one session per platform, in parallel

Each session moves its platform's agent code into the plug, adds the doors it needs to `SOCKET.md` and to branch
`gpui-agent`, and proves the platform on a real machine.

| Platform | Machine | Code to move | How it is run today |
|---|---|---|---|
| macOS | macOS | `nsh-platform-macos/src/gpui_agent.rs`, our edits to `src/agent.rs`, `tests/native/*agent*` | `docs/dev/tooling.md` "macOS GPUI-agent development", `docs/dev/macos.md` |
| Linux | Linux, under Xvfb | our edits to `nsh-platform-linux/src/agent.rs` | `GPUI_AGENT_REPORT.md` "Environment and launch"; `docs/dev/linux.md` |
| Windows | Windows | our edits to `nsh-platform-windows/src/agent.rs` | `docs/dev/windows.md` |
| Android | Linux or Windows with the emulator | our edits to `nsh-platform-android/src/agent.rs` and `TestAgent.java` | `docs/dev/tooling.md` "Android GPUI-agent development" |
| iOS | macOS with the Simulator | our edits to `nsh-platform-ios/src/agent.rs` | `docs/dev/tooling.md` "iOS Simulator GPUI-agent development", `docs/dev/ios.md` |
| Web | any | `nsh-cli/src/web_agent.rs`, and the web synthetic-input commits listed in the upstream handover as theme `S01` | `docs/dev/tooling.md` "Web GPUI-agent development", `docs/dev/web.md` |

The run is the same everywhere: start the gallery with the agent on, then drive it with the `gpui-agent` CLI.

```sh
export GPUI_AGENT_TOKEN="$(openssl rand -hex 32)" GPUI_AGENT_ADDR=127.0.0.1:17421
GPUI_AGENT=1 nsh run examples/gallery --target <platform> &
gpui-agent hello
gpui-agent snapshot --pretty
gpui-agent click <target-id>
gpui-agent screenshot --out gallery.png
```

Machine setup for the Devin sessions is in `hexuria/buwiz-forms`: `docs/platforms/devin-environment.md` and
`docs/platforms/devin-blueprint.yml` (Linux with Android, macOS with the iOS Simulator, Windows with Android).

A platform is done when, built from the plug and branch `gpui-agent`:

- every operation `GPUI_AGENT_REPORT.md` and that platform's docs say works still works, shown by the CLI output;
- the tests that existed for it pass (the macOS native agent tests, the gallery `agent-*` tests where they run);
- the platform's ordinary `nsh test` still passes, so the socket did not change the test agent;
- the platform's upstream checks pass (`CONTRIBUTING.md` "Checks": the clippy run for that platform).

Sessions touch different platform crates, but all of them add rows to `SOCKET.md` and may need the same shared
door. Put shared doors in `nsh-testing` or `nsh-core` once, in Stage 0's style, and rebase on each other often.

### Stage 2: put it together

1. Build `buwiz-next`: branch `gpui-agent`, then our non-agent commits unchanged. Of the 86 commits that are not
   agent work, 24 are agent support (themes `S01` to `S03` in the upstream handover's `data/themes.tsv`: web
   synthetic input, semantic tree queries, event provenance). Those belong to this move: each ends in the plug or
   as a door on `gpui-agent`. The other 62 go onto `buwiz-next` as they are. Check that for the files those 62
   touch, `buwiz-next` is byte-identical to `buwiz`.
2. Measure the socket: `git diff v0.6.1 gpui-agent`, lines and files per crate, against the 16,195 lines of agent
   work it replaces.
3. In `hexuria/buwiz-forms`, open a PR that moves `nsh/` to the plug and to the `buwiz-next` pin: `nsh/Cargo.toml`
   (today a path dependency on `nsh-platform-macos` with feature `gpui-agent-server`; add the plug and the
   `[patch]` that points its NSH at the app's checkout), `NSH_PINNED_REF` in `.github/workflows/nsh.yml`, and the
   setup scripts that clone `hexuria/nsh`.
4. Prove the app: `just nsh-agent-check`, `just nsh-run-agent <target>` on each platform, and the window recipes
   under `agent-recipes/ui/`.

### Stage 3: rehearse the catch-up

Rebase `gpui-agent` onto upstream's current `main` on a throwaway branch. Record how many door commits
conflicted and how long it took, against the same rebase of `buwiz`. This number is the point of the whole plan.
Do not push the rehearsal over `gpui-agent` or `buwiz-next`.

### Stage 4: gpui-agent 0.4.0

Move the plug from `gpui-agent =0.3.0` to 0.4.0 (on crates.io since 2026-11-23), in its own PR, with every
platform's run repeated.

## Done when

- [ ] `hexuria/nsh-gpui-agent` holds the adapter, the agent actions and every platform's agent code.
- [ ] On branch `gpui-agent`, the only difference from the upstream tag is the doors, no crate depends on the
      `gpui-agent` library, and `SOCKET.md` lists every door.
- [ ] `buwiz-next` is `gpui-agent` plus the 62 non-agent commits, unchanged.
- [ ] The plug's README names the NSH branch and commit to use, and how an app patches it in.
- [ ] Each of the six platforms passes its Stage 1 checks on a real machine, with the output kept.
- [ ] `hexuria/buwiz-forms` has an open PR onto the plug and the `buwiz-next` pin, and the app passes Stage 2's
      checks.
- [ ] The Stage 3 rehearsal is reported with its numbers.
- [ ] Anything that stayed in the fork beyond a door is listed with the reason.
