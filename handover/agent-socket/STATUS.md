# STATUS — gpui-agent move + buwiz-forms issue #269 (updated 2026-11-24 ~00:45 UTC by session devin-7eb7d5a448484540b772f6556a2877a5)

This file is the cold-start handover after the NSH orchestrator Bot (session
`c411d19b0ffd4589a5d65b8e7858d28c`) and all six quota-paid platform sessions ran
out of ACU quota. Orchestration has moved to free session
`devin-7eb7d5a448484540b772f6556a2877a5` (this box, "NSH"). Everything below is
verified state, not plans. Read `GOAL.md` and `PLAN.md` in this directory first
— the picks there are final.

Resume instructions for the next session are in `RESUME.md` next to this file.

## Rules that still bind everyone

From GOAL.md (verbatim where quoted):
- Never set `BIR_SFTP_LIVE`. Safety env for every run: `BUWIZ_NO_KEYCHAIN=1
  EBIR_TEST_ENV=1 BIR_AUTH_BYPASS=1 BIR_SFTP_DRY_RUN=1`.
- "Not allowed: merging; a real release tag; anything that becomes the latest
  release; adding or reading repository secrets; uploading to TestFlight or
  App Store Connect; editing NSH; deleting `rox-mini-app-round2` or any branch
  that is not yours; force-pushing anyone else's branch; renaming
  `agent::APP = "bir-shell"`."
- hexuria/nsh is read-only except through the door commits on `gpui-agent` —
  never open a PR against it, never push to main. Owner-only gates in GOAL.md
  stay owner-only (no merges, no real tags, no NSH_UPDATE_KEY; draft
  pre-releases deleted after use).
- Push/PR auth: bind `GITHUB_TOKEN=secret:personal:GITHUB_TOKEN` and push to
  `https://x-access-token:$GITHUB_TOKEN@github.com/...` — the git proxy 403s
  without it. The token CANNOT open PRs (no pull-request write): someone else
  must open PRs from the branch-compare links, or grant the token that scope.
- "When orchestrating sub-agents, require each to report with video or
  screenshot proof plus text" (uriah's standing rule).
- Use the four free platform sessions first, never new agent sessions:
  windows `5171718ab21d47b785af639bda550158` (has Android),
  linux `a32c98e9a2ff4f3197f3cb8b66227b07`, linux
  `366d75587ebd4b1183f50aca1bcd3353` (this box), macos
  `e0730bfce0f54d80a275b98a77bc54b6` (has iOS). Messaging a session on another
  org quota may return HTTP 403 out_of_quota — that means the Bot is dead, not
  busy.

## Dead sessions (out_of_quota, do not wait for them)

Orchestrator `c411d19b0ffd4589a5d65b8e7858d28c` (NSH Bot) and its six Stage-1
platform children: android `89e6cf31…`, ios `6f5674fa…` (PR #3), web
`80660509…` (PR #4), windows `d920f5e3…`, macos `927ad5e2…`, linux
`e230d1ec…`. Their pushed work survives on the remotes below; uncommitted
work is gone.

## Program A — the gpui-agent move (hexuria/nsh + hexuria/nsh-gpui-agent)

### Key refs
- Upstream pin: `v0.6.1` = `2f51d8611` (fetched from longbridge/nsh).
- `gpui-agent` tip = `56add34a019022de6433dcf6fa62b345d33a3342` — v0.6.1 +
  **all door commits for all six platforms (40 doors in SOCKET.md)**. macOS
  doors landed 2026-11-24 by cherry-picking `devin/stage1-macOS-doors`
  (`cab9140b9`, 18 commits, conflict in SOCKET.md only).
- `buwiz` tip = `70d8ff26b9e9dccb17a908de97cd4a3f3e381276` (`NSH_PINNED_REF`
  in buwiz-forms `.github/workflows/nsh.yml`).
- longbridge/nsh `main` = `958262185` (moved since `167aa3827`).
- iOS door pin used for checks: `ca714e1fb`.

- Plug repins (all now `56add34a`): linux `91de1a7` (PR #5 open), windows `d21e3a9`, android `3a35f66`, macos `bd0d241`, web `b7fe119` (PR #4), ios `3474e96` (PR #3), bugfixes `1776bbb` (PR #2).
- Plug repo `main` = `22e8db6e` (stage-0 move merged via PR #1).

### Socket size check (Task-13 audit by windows session + rerun here)
- `git diff v0.6.1 gpui-agent` = 38 files, +3,786/−1,372 at 39 doors; the 40th
  platform (macOS, 17 door rows) adds ~+283/−71 in nsh-platform-macos plus
  SOCKET.md rows. Rerun the full count at the final pin for the report.
  Per the TASK-13 audit every line maps to a SOCKET.md row; ~1,336 of nsh-cli's
  lines are a byte-identical lib.rs move, so real new code ≈ +1,850.

### Stage-1 platform state
| Platform | Doors on gpui-agent | Plug branch | Plug PR | Check status |
|---|---|---|---|---|
| web | yes (cli/dev doors incl. 089981575) | `web` | #4 open | TASK-10 PASSED on Windows Chrome (both modes; paired-tab screenshot is a documented limit) |
| ios | yes | `devin/1795407430-ios-stage1` | #3 open | TASK-11 PASSED on iPhone 17 sim (all ops); README gap list reported |
| linux | yes | `devin/linux-agent` @ 91de1a7 repinned to 56add34a | **#5 OPEN** (mergeable) | a32c TASK-16 passed all ops (3 README gaps); linux parity CLEAN: 217 common tests v0.6.1 vs 56add34a in docker (40-min timeout each, same prefix), 2 timing-flake flips, no systematic new failures |
| windows | yes | `devin/windows-agent` @ d21e3a9 repinned | **#6 OPEN** (mergeable) | clean check DONE: all ops delivered on real window (hello/snapshot/type/set-value verified; Quit modal proven; +3 README gaps — port-held silent start, modal not in tree, key/keybinding invocation docs) |
| android | yes | `devin/1795407849-android` @ 3a35f66 repinned | none | TASK-14 in flight on 5171 — build chain proven end-to-end (AVD gpui35, WHPX, NshDev.apk 217MB); emulator second boot in progress; 9 README gaps reported |
| macos | yes | `devin/stage1-macos` @ bd0d241 repinned | none | TASK-15 DONE clean: all ops verified; gallery 391/73 vs stock 390/74, zero new failures |

### Known defects found so far
- iOS fixture comparison (TASK order to e073): 8 of 93 iOS native fixtures fail
  at `1822d2e15`/`68a4a99ec`. 7 are pre-existing on `buwiz` (MenuScroll,
  FocusRow, FocusTransfer, LargeTitle, 2× navigation/Keyboard, Selection).
  ~~1 regression: keyboard/Composer~~ **RESOLVED — environmental**: e073 found
  the iPhone 17 sim had a hardware keyboard connected; buwiz fails identically
  there. Fixture fix on `devin/1795423559-composer-hwkeyb-fix` off `56add34a`
  (compare vs gpui-agent). NOTE: `nsh test --target ios` results are
  sim-keyboard-state-dependent — re-check other pre-existing failures on a sim
  with Connect Hardware Keyboard off.
- 8 agent bugs from Devin Review on PR #1 were fixed on
  `devin/1795413363-agent-bugfixes` = PR #2 (open). Two remaining web findings
  behave the same on buwiz → follow-ups, not blockers.

### Rehearsals done on this box (TASK-9, TASK-12; worktree `/home/ubuntu/repos/nsh-stage3`)
- TASK-9 (62 non-agent buwiz-next commits onto gpui-agent tip): **32 clean /
  30 conflict** at 24/39 doors; **29 clean / 33 conflict** re-run at
  `56add34a` (macOS doors added ~3 context conflicts). Only ONE real
  door conflict: `abe8d514c` (B07 ios view.rs, door rewrote `Inner::load`).
  The rest are missing skipped-commit context (~17 cascade from
  `ad9c1f10e`/B15 head leaving `global_menu.rs` absent; `f0d2d183e` is a
  byte-identical duplicate of `815d991d3`, not a real conflict).
- TASK-12 (rebase gpui-agent onto longbridge/main): at 24 doors 21 clean /
  3 conflict; at 39 doors **27 clean / 12 conflict**; re-run at 57 door
  commits (incl. docs/meta commits) vs moved main `958262185`: **41 clean /
  16 conflict** — all mechanical same-region overlap (nsh-cli churn hits
  cli+android doors; SOCKET.md docs commits cascade; windows surface.rs).
  Comparison `buwiz` → same main: **60 clean / 196 conflict**. This is the
  number the socket exists to beat.
- Scratch state kept: branches `stage3-gpui-agent2`, `buwiz-next-rehearsal2`
  in worktree `~/repos/nsh-stage3`; lists in `/tmp/replay_list.txt` (62 SHAs),
  `/tmp/door_list2.txt` (39 SHAs), `/tmp/stage3_gpui2.txt`, `/tmp/replay2.txt`.
  `buwiz-next-rehearsal` also exists in `~/repos/nsh`. NONE of this is pushed.

### Outstanding move work (Stages 2–4 of PLAN.md)
1. ~~macOS doors must land~~ DONE — `gpui-agent` = `56add34a`, all 40 doors.
   TASK-9 + TASK-12 re-run: see numbers above.
2. TASK-15 (macOS desktop check) rebuilding on e073 at new pin; TASK-14
   (Android check) in flight on 5171 — emulator booting, build chain proven.
   Wait for both before declaring Stage 1 done.
3. Open plug PRs for `devin/linux-agent`, `devin/windows-agent`,
   `devin/1795407849-android`, `devin/stage1-macos` (token cannot open PRs —
   needs a human or a PR-write-capable token). Review and land #2/#3/#4.
4. ~~Build `buwiz-next`~~ DONE — branch `buwiz-next` pushed on hexuria/nsh:
   50 of 62 picks clean, 11 resolved (3-way per-file merge; door files merged
   by re-applying the door diff onto buwiz's file), 1 skipped (f0d2d183e
   byte-identical dup). Fixups: `afae67f7` merge resolutions + linux
   Cargo.toml (global-menu feature kept, agent deps out). nsh-core/nsh-testing
   `cargo check` clean. Files the 62 touch: 53/75 byte-identical to buwiz;
   22 differ — all verified as door diffs or agent/upstream-backport regions
   (e.g. placing() from #2347, agent doc sections). NOT yet compiled per
   platform or run-tested.
5. ~~Composer regression~~ RESOLVED — see Known defects above.
6. Final: SOCKET.md audit rerun, update buwiz-forms `NSH_PINNED_REF` to the
   final socket pin, delete `crates/bir-shell` per #269 PR 2.

## Program B — hexuria/buwiz-forms issue #269 (PR 1 `feat/nsh-release`)

- PR 1 assembly is owned by track-A session `devin-5a27540f503c4840876d5dc8346efcf5`
  (buwiz-forms Bot `bot-641ea1b0c2a0430da76505356de38a7b` coordinates — that
  Bot is also out_of_quota; could not receive my report).
- **ALL Linux gates on `feat/nsh-release` @ `7f2db8cb` PASSED** (reported to
  the dead Bot; repeat to whoever picks up): `nsh-check -v`, `nsh-build`,
  `nsh-test` (136 cargo + 187/187 TSX files, ~1,700 tests), `core-plain-test`
  (751), `cargo test -p bir-cli` (3), touched Python script tests (86 ran,
  18 skipped). Run inside Docker sidecar `buwiz-nsh-2404` via
  `~/BuwizNsh/nsh2404.sh` — host is Ubuntu 22.04, lacks GTK 4.14/WebKitGTK 6.
- Windows free session is doing track-E install proof (Setup.exe/MSI install,
  launch with safety flags, sign-in mock, form open, MSIX, uninstall) plus
  TASK-14. macOS free session awaits PR 1 for the notarized-DMG e2e check.
- Other tracks' results are in the buwiz-forms Bot's dead session; the open
  state is whatever `feat/nsh-release` and its CI show on
  https://github.com/hexuria/buwiz-forms/issues/269.

- buwiz-forms Stage 3 branch pushed: `devin/stage3-plug-move` on hexuria/buwiz-forms (plug dep @ plug `devin/stage2-integration` 1e85cf6 + [patch] to app checkout + NSH_PINNED_REF = buwiz-next 04955773). gpui-agent dual-versioned (0.3.0 plug / 0.4.0 app) until Stage 4.

- Stage 2 acceptance (e073, macOS, devin/stage3-plug-move): app gates all green (nsh-check/build/test), BUT plug window plane binds an empty 0×0 tree (no UI op lands; suspected missing nsh_gpui_agent_macos::install() in the app wiring) and the app data plane never bound. Investigation dispatched to e073.
