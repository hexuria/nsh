# RESUME — how to pick this work up in a fresh session

Give the new session this prompt (adapt only the bracketed parts):

---

You are continuing the gpui-agent move out of the hexuria/nsh fork plus the
buwiz-forms issue #269 release work. Everything you need is in git.

1. Clone https://github.com/hexuria/nsh (or reuse an existing clone) and check
   out branch `handover/agent-socket-status`. Read, in order:
   `handover/agent-socket/GOAL.md`, `handover/agent-socket/PLAN.md`, then
   `handover/agent-socket/STATUS.md` (the verified current state — where every
   platform, PR, defect and rehearsal stands). The "Read first" list in
   GOAL.md and all PLAN.md picks/decisions are final.
2. Follow GOAL.md's Allowed / Not allowed lists exactly: never set
   BIR_SFTP_LIVE; run everything with `BUWIZ_NO_KEYCHAIN=1 EBIR_TEST_ENV=1
   BIR_AUTH_BYPASS=1 BIR_SFTP_DRY_RUN=1`; hexuria/nsh is read-only except door
   commits on `gpui-agent`; no merges, no real release tags, no repository
   secrets, no force-pushes to other people's branches.
3. For pushes bind `GITHUB_TOKEN=secret:personal:GITHUB_TOKEN` and use
   `https://x-access-token:$GITHUB_TOKEN@github.com/...` (the git proxy 403s
   otherwise). The token cannot open PRs — prepare compare URLs and ask the
   user to click them, or ask for PR-write scope.
4. Orchestrate on the four free platform sessions first (never paid child
   sessions): windows `5171718ab21d47b785af639bda550158`, linux
   `a32c98e9a2ff4f3197f3cb8b66227b07`, linux
   `366d75587ebd4b1183f50aca1bcd3353`, macos
   `e0730bfce0f54d80a275b98a77bc54b6`. Check each is free before assigning.
   Sessions on another org's quota may 403 when messaged — that means dead,
   not busy.
5. Open items, in order (details + owners in STATUS.md):
   a. Wait for TASK-14 (Android check, windows session) and TASK-15 (macOS
      check, macos session) to report, then land macOS doors
      (`devin/stage1-macos-doors` @ `cab9140b9`) onto `gpui-agent`.
   b. Re-run the TASK-9/TASK-12 rehearsals against the new tip (method + the
      exact commands' shape are described in STATUS.md; use a throwaway
      worktree and scripted `cherry-pick -x`, abort-on-conflict).
   c. Open plug PRs for `devin/linux-agent`, `devin/windows-agent`,
      `devin/1795407849-android`, `devin/stage1-macos`; drive review on #2,
      #3, #4.
   d. Fix the one real regression found: keyboard/Composer iOS fixture passes
      on `buwiz` but fails at pin `68a4a99ec` (tap never focuses input).
   e. Build `buwiz-next` = `gpui-agent` + the 62 non-agent commits per PLAN.md
      Stage 4 (62-SHA list kept at `/tmp/replay_list.txt` on session
      366d7558's box, or regenerate from
      `handover/upstream-prs/data/candidates.tsv` minus the 24 S01–S03 rows).
   f. Report on https://github.com/hexuria/buwiz-forms/issues/269 and on this
      move per GOAL.md's final-report lists.
6. Report to uriah only when a stage is ready for final review, or on a
   blocker only he can clear. Do not send progress chatter.

---

## Machine-specific notes for session 366d7558's box (Linux, Ubuntu 22.04)

- buwiz-forms clone: `/home/ubuntu/repos/buwiz-forms` (on `feat/nsh-release`).
- nsh clone: `/home/ubuntu/repos/nsh` (on `buwiz`); extra worktrees
  `/home/ubuntu/repos/nsh-pinned` (@70d8ff26b) and `/home/ubuntu/repos/nsh-stage3`.
- NSH app gates can't build on the host (no GTK 4.14 / WebKitGTK 6.0). Docker
  sidecar `buwiz-nsh-2404` is built; run `~/BuwizNsh/nsh2404.sh <cmd>` with cwd
  = the repo's `nsh/` dir (`--network host`, HOME mounted). `just nsh-check -v`
  / `nsh-build` / `nsh-test` map through it.
- NSH CLI `~/BuwizNsh/tools/bin/nsh` v0.6.1 exists (built from nsh-pinned via
  `cargo install --locked --path crates/nsh-cli --root ~/BuwizNsh/tools`).
- Useful scratch: `/tmp/TASK9-report.md`, `/tmp/replay_list.txt`,
  `/tmp/replay2.txt`, `/tmp/stage3_gpui2.txt`, `/tmp/door_list2.txt`,
  `/tmp/gate-core-plain.log`, `/tmp/gate-bir-cli.log`.
