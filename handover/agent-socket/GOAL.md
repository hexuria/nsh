# Goal: the NSH fork keeps a small socket, our gpui-agent work lives in our own repository

Move every piece of gpui-agent logic out of `hexuria/nsh` into a new repository of ours, leaving in the fork only
the small public doors that logic needs. Do it for all six platforms, proven on real machines. Nothing about how
the agent behaves changes.

The decision, the measurements and the stages are in `PLAN.md` beside this file. The owner's picks and the
"Decisions made to fill gaps" there are final: do not re-ask them.

## Read first, fully

1. `handover/agent-socket/PLAN.md` and the files in `handover/agent-socket/data/`.
2. In this repository on branch `buwiz`: `AGENTS.md`, `GPUI_AGENT_REPORT.md`, and the "GPUI-agent development"
   sections of `docs/dev/tooling.md`, `docs/dev/macos.md`, `docs/dev/linux.md`, `docs/dev/windows.md`,
   `docs/dev/ios.md`, `docs/dev/web.md`.
3. Upstream's rule for what belongs in NSH: `CONTRIBUTING.md` on `longbridge/nsh` `main`, section "Scope: what NSH
   accepts". It is why this work is leaving the fork, and it sets the bar for a door.
4. In `hexuria/buwiz-forms`: `nsh/AGENTS.md` (how the app uses the agent), `docs/platforms/devin-environment.md`
   and `docs/platforms/devin-blueprint.yml` (how the machines are set up).

## Where

| What | Where |
|---|---|
| The code being moved | `hexuria/nsh`, branch `buwiz`, at `70d8ff26b` (NSH 0.6.1 plus our work) |
| The NSH the plug is built for | `hexuria/nsh`, new branch `gpui-agent`: upstream tag `v0.6.1` plus the doors, nothing else |
| The NSH the app pins | `hexuria/nsh`, new branch `buwiz-next`: branch `gpui-agent` plus our non-agent commits |
| The plug | `hexuria/nsh-gpui-agent` (public, already created, holds a starting `README.md`) |
| The app that uses it | `hexuria/buwiz-forms`, directory `nsh/` |
| Upstream | `longbridge/nsh` (read only for this goal) |
| This handover | `hexuria/nsh`, branch `feat/nsh-agent-socket`, directory `handover/agent-socket/` |

## How

Follow `PLAN.md`'s stages in order: 0 (foundations, one session), 1 (one session per platform, in parallel), 2
(put it together and move the app), 3 (rehearse a rebase onto upstream), 4 (gpui-agent 0.4.0).

For Stage 1, run one session per platform on the machine that platform needs: macOS (also iOS Simulator), Linux
(also Android emulator, and Linux desktop under Xvfb), Windows. Web runs anywhere. Each session reads this file and
`PLAN.md`, takes one row of the Stage 1 table, and reports against that row's done list.

Rules for every session:

1. **A door only exposes.** If a change to the fork contains logic, it belongs in the plug. If you cannot avoid
   it, write the reason in `SOCKET.md`.
2. **Move, then prove, then stop.** Do not improve the code you move. A bug you find goes in the report and, at
   most, in a separate commit after the move passes.
3. **Prove on the real target.** A platform crate compiles as an empty crate on the wrong system, so a green build
   on the wrong machine proves nothing. Keep the command output for the report.
4. **The ordinary test agent must be untouched.** `nsh test` on each platform passes exactly as on `buwiz`.
5. **Small commits on branch `gpui-agent`, one door each**, with upstream's commit style: `<area>: <What it does>`, no
   `fix:` or `feat:` prefixes, no mention of gpui-agent.
6. Commit style, formatting and checks are upstream's: `cargo +nightly fmt --all`, `cargo test --workspace`, and the
   platform clippy run in `AGENTS.md` "Verification".

## Allowed without asking

- Push to `hexuria/nsh-gpui-agent` (it exists; `main` is its working branch until the first release).
- Create and push branches on `hexuria/nsh` and `hexuria/buwiz-forms`, and open pull requests there (draft until
  proven).
- Build, run and test on the session's machine, emulator and simulator.

## Not allowed

- Merging any pull request.
- Pushing to, rebasing or force-pushing `buwiz` on `hexuria/nsh`, or `main` on `hexuria/nsh` and
  `hexuria/buwiz-forms`.
- Opening issues or pull requests on `longbridge/nsh`. Offering doors upstream is the other handover's job; give it
  `SOCKET.md`.
- Publishing a crate.
- Changing `hexuria/gpui-agent`.
- Changing the non-agent commits. They ride along unchanged.
- Running the eBIRForms app against a real BIR server or the owner's real database. Use the scratch databases and
  the mock account that `hexuria/buwiz-forms` `nsh/AGENTS.md` describes.

## If something cannot be done as planned

Build the closest thing that keeps the pick's intent, write why in `SOCKET.md` and the report, and keep going. Do
not stop to ask. The one exception: if Stage 0 shows the socket cannot be small (the plug would need most of
`nsh-testing`'s internals), stop after Stage 0 and report that, with the door list that would be needed.

## Finish with one report

- `hexuria/nsh-gpui-agent`, and branches `gpui-agent` and `buwiz-next`, with links.
- The socket: every door, its size, and the total against the 16,195 lines of agent work it replaces.
- Per platform: what was run, on which machine, and the result, with the output.
- What stayed in the fork that is not a door, and why.
- The `hexuria/buwiz-forms` pull request and the app's results.
- The Stage 3 rehearsal: conflicts and time for `gpui-agent` against the same rebase of `buwiz`.
- Anything you found that the owner should decide.
