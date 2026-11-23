# Goal: land our non-agent NSH changes upstream, one small pull request at a time

Our fork of NSH (`hexuria/nsh`, branch `buwiz`) carries 86 commits that are fixes and features for the framework
itself. Get as many as upstream will take merged into `longbridge/nsh`, so the fork stops carrying them. Keep a
ledger of what happened to each.

The sorted candidates, upstream's rules, our record there and the method are in `PLAN.md` beside this file. Its
"Decisions made to fill gaps" are final: do not re-ask them.

## Read first, fully

1. `handover/upstream-prs/PLAN.md` and the files in `handover/upstream-prs/data/`.
2. On `longbridge/nsh` `main`: `CONTRIBUTING.md` (all of it), `AGENTS.md`, `.github/pull_request_template.md`,
   `docs/dev/commands.md`, and the `docs/dev` page for each area you touch.
3. Three of our merged upstream pull requests, to see what a good one looks like there: #1511, #1513 and #2283 on
   `longbridge/nsh`. And the three closed ones, with their threads: #449, #1500, #112.

## Where

| What | Where |
|---|---|
| Our changes | `hexuria/nsh`, branch `buwiz`, at `70d8ff26b` (NSH 0.6.1 plus our work) |
| Upstream | `longbridge/nsh`, branch `main` |
| Work branches | `hexuria/nsh`, one per pull request, `up/<area>-<short-name>`, each started from upstream `main` |
| This handover and the ledger | `hexuria/nsh`, branch `feat/nsh-upstream-prs`, directory `handover/upstream-prs/` |
| Machines | Devin sessions on Linux, macOS and Windows, with the Android emulator and the iOS Simulator. Setup: `hexuria/buwiz-forms` `docs/platforms/devin-environment.md` and `devin-blueprint.yml`. |

Upstream pull requests are opened from the GitHub account that opened our earlier ones (`hexuria`). The session
needs that account's access; if it does not have it, prepare the branch and the description and say so in the
ledger instead of opening the pull request from another account.

## How

Work the themes in `PLAN.md`'s order, following "The work, per candidate" there. Independent themes can run in
parallel sessions, each on the machine its platform needs: Windows themes on Windows, macOS and iOS on macOS, Linux
and Android on Linux. Keep at most five of our pull requests open upstream at once.

Rules for every session:

1. **Check upstream first.** A candidate that upstream already fixed is a ledger row, not a pull request.
2. **One pull request, one problem.** If the description needs the word "also", split it.
3. **Rebuild on upstream `main`.** Read our commit to learn what it did, then make the change in today's code.
4. **Prove it on the platform.** A platform crate is empty on the wrong system; run the platform's clippy and the
   gallery demo on its machine, and attach screenshots for anything visible.
5. **New public API means an issue first.** No pull request for an N theme, or for a door, before a maintainer
   agrees on the issue.
6. **Write for the maintainer.** Say what was wrong for a user of NSH and what is right now. Never mention
   gpui-agent, our app, or our fork's needs.
7. **Take no for an answer.** A declined issue or a closed pull request is recorded with its reason and left.

## Allowed without asking

- Create and push `up/...` branches on `hexuria/nsh`, and commit to `feat/nsh-upstream-prs` (the ledger).
- Open issues and pull requests on `longbridge/nsh` for the B themes, and issues for the N themes, within the
  five-open limit.
- Reply to review comments and push follow-up commits to our own pull request branches.
- Build, run and test on the session's machine, emulator and simulator.

## Not allowed

- Any pull request or comment upstream that carries gpui-agent work or names it.
- Reopening or resubmitting #449, #1500 or #112, or theme X01.
- Opening a pull request for an N theme or a door without a maintainer's agreement on its issue.
- Pushing to, rebasing or force-pushing `buwiz`, `buwiz-socket` or `main` on `hexuria/nsh`.
- Merging anything, anywhere. Upstream's maintainers merge through their queue.
- Pinging maintainers repeatedly, or arguing a declined change.
- Starting the doors track before `SOCKET.md` exists on `buwiz-socket` and its doors are proven.

## If something cannot be done as planned

If a candidate cannot be reproduced on upstream `main`, or cannot be tested on a machine you have, write that in
the ledger and move to the next one. Do not send a change you could not run.

## Finish with one report

- The ledger, complete.
- Totals: merged, already upstream, open, declined, not sent, with the lines of our fork each total accounts for.
- For each open pull request or issue: the link and what it is waiting on.
- What the next catch-up of the fork can drop.
- What you learned about what upstream takes and refuses, in a few lines, for the next round.
