# Plan: send our non-agent NSH changes upstream

Branch `feat/nsh-upstream-prs` of `hexuria/nsh`. This branch carries the handover and the ledger only; it is not
merged anywhere. The companion handover, for moving the gpui-agent work out of the fork, is branch
`feat/nsh-agent-socket`.

Everything under "Measured" was measured on 2026-11-23 on `hexuria/nsh` branch `buwiz` at `70d8ff26b`, against
upstream tag `v0.6.1` and upstream `main` at `67ed2b9e2`. Upstream lands about 95 commits a day, so check each
candidate against the current `main` before working on it.

## The problem

`hexuria/nsh` branch `buwiz` is upstream NSH 0.6.1 plus 242 commits of ours. 156 of them are gpui-agent work, which
upstream declined and which the other handover moves out. The other 86 commits (5,963 lines) are fixes and features
for the framework itself. As long as they exist only in our fork, we carry them through every upstream release.

Each one that upstream merges is one we stop carrying. That is the goal: shrink the fork by landing our changes
where they belong.

## What upstream accepts

Read `CONTRIBUTING.md` on `longbridge/nsh` `main` before anything else, all of it. The parts that decide this
plan:

- **One pull request does one thing**, with the smallest diff that does it.
- **Taken without a prior issue, in order of preference:** bug fixes on any platform; performance; refinements of
  existing behavior; completing existing components; bringing an existing feature to every platform where a backend
  lacks it.
- **A new feature needs an issue and a maintainer's agreement first.** It is accepted only if apps cannot build it
  themselves (through Rust extension points or as an nshm), many apps need it, it works the same on every platform,
  and its cost is measured. "A pull request that adds a feature without that agreement may be closed without a
  review of its code."
- **Anything public goes in the description** under `## Public API`, with signatures. Changes to existing public
  items go under `## Breaking Changes`. What apps use is never broken once released, and a new API comes with a case
  in `tests/compat/`.
- **Titles and commits:** `<area>: <What the change does>`, imperative, capital first letter, no trailing period,
  about 72 characters, one area only, no `fix:` or `feat:`.
- **Checks before opening:** `cargo test --workspace`, `cargo +nightly fmt --all`, `cargo clippy --workspace
  --all-targets`, and the clippy run for each platform the change touches, on that platform. Platform code is run on
  a simulator, emulator or device; UI changes come with screenshots.
- **AI-written pull requests are welcome.** The contributor answers for the result.
- Pull requests use GitHub's merge queue and are squash merged.

The pull request template is `.github/pull_request_template.md`. Fill every section that applies and delete the
ones that do not.

## Our record upstream

From the GitHub account `hexuria` (`data/our-upstream-prs.tsv`): 20 pull requests, **17 merged, 3 closed**.

| Closed PR | Why | Lesson |
|---|---|---|
| #449 `macos: gpui-agent control plane + synthesized input invokes` | "Not ready to accept this feature." | A feature for one app's need. Do not send gpui-agent work, under any name. |
| #1500 `macos: Spread a batch that makes hundreds of views out of sight over frames` | Upstream solved the same problem in the core, for every platform (#1600), and thanked us for the measurement. | A macOS-only layer loses to a core fix. Prefer the fix that serves every platform, and check `main` first. |
| #112 `core: Support context menu requests on every platform` | Went stale, replaced by upstream's own #253. | Small and finished beats large and waiting on a platform we could not test. |

The 17 merged ones are all fixes and refinements, most of them on macOS, a few in the core and the CLI.

## Measured: the 86 commits, sorted

`data/candidates.tsv` lists every commit with its theme. `data/themes.tsv` sums each theme. The class is a first
reading from commit subjects and file lists; confirm it by reading the code. "In upstream" is the share of a
commit's added lines that already appear in the same file on upstream `main`: a rough signal, not proof.

**Send as they are: no issue needed (2,953 lines)**

| Theme | Class | Commits | Lines | In upstream | What it is |
|---|---|---|---|---|---|
| B01 | fix | 2 | 88 | ~48% | core: folded text and the node quota |
| B02 | fix | 1 | 90 | ~56% | animation: leaf completions while programs advance |
| B03 | fix | 3 | 42 | ~3% | windows: resize by the client area, counting the menu bar |
| B05 | fix | 1 | 13 | 0% | windows: take the keyboard back from web views when a press lands outside them |
| B06 | fix | 1 | 9 | 0% | macos: report Up and Down from a text input |
| B07 | fix | 1 | 1 | 0% | ios: read the dev session from the held view state on reload |
| B08 | fix | 2 | 93 | ~3% | js: retire replaced app-menu callbacks |
| B09 | refine | 1 | 43 | 0% | macos: draw views at the scale they are shown at |
| B10 | refine | 1 | 128 | ~29% | macos: keep the lines canvases set their texts in |
| B11 | refine | 2 | 202 | ~18% | macos: draw content about to scroll into view ahead of time |
| B12 | refine | 1 | 58 | ~6% | macos: pause an embedded app while it is unseen |
| B13 | refine | 2 | 293 | ~1% | macos: key order between a hosted app view and its host |
| B04 | every platform | 1 | 195 | 0% | windows: a web view opens the file page its app loaded. Upstream has the macOS half from our #1496. |
| B14 | every platform | 4 | 51 | ~17% | embedding: `AppView::set_error_handler` on macOS, Linux and Windows |
| B15 | every platform | 15 | 1,647 | ~1% | linux: export the application menu to KDE Plasma's Global Menu |

B01 and B02 overlap with upstream by about half: check first whether upstream already fixed them another way. B15
is large; it needs splitting, and may be judged a new direction rather than a platform catching up. Ask in an
issue before spending time on it.

**New public API: open an issue first and wait for agreement (1,295 lines)**

| Theme | Commits | Lines | What it is |
|---|---|---|---|
| N01 | 7 | 533 | desktop: say whether the menu is in a global menu bar and whether a tray shows (`Tray.available()` and the like) |
| N06 | 6 | 508 | engine and macos: the jank line and the performance monitor's controls |
| N03 | 2 | 85 | core: React Native handler names (`onChangeText` and others) |
| N05 | 1 | 85 | macos: `run_app` opens its window without taking focus |
| N02 | 2 | 49 | core: `ElementRef::item_offset` |
| N04 | 3 | 35 | keymaps: bind a menu's reserved keystroke |

For each, the issue has to answer upstream's four points: why an app or an nshm cannot do it, who else needs it,
that it works on every platform, and what it costs. N01 already has stubs for the web and for embedded views,
which helps the "every platform" point. If a maintainer says no, record it in the ledger and stop: that change
stays in the fork or moves to the app.

**Not for upstream (1,715 lines)**

| Theme | Commits | Lines | Why |
|---|---|---|---|
| X01 | 1 | 763 | macos: spread a batch over frames. Upstream closed our #1500 and solved it in the core. Drop it from the fork at the next catch-up; first confirm upstream's version covers our case. |
| S01 | 16 | 783 | web: synthetic input for a driven tab. Agent support; it goes with the agent extraction. If a commit turns out to fix upstream's own `nsh test --target web`, lift that one out as a fix. |
| S02 | 7 | 97 | testing and dev: semantic tree queries, the menu shortcut catalog. Agent support. |
| S03 | 1 | 48 | macos: do not count an event another process posted as the person's. Agent support. |
| H01 | 2 | 24 | clippy and docs for our own debug host. |

## Decisions made to fill gaps

Change any of these before starting; do not re-ask them mid-run.

1. **Order:** fixes (B01 to B08), then refinements (B09 to B13), then the every-platform themes (B04, B14, then
   B15 after its issue), then issues for the new-API themes.
2. **At most five of our pull requests open upstream at a time.** The maintainer merged five of ours in one day on
   2026-11-10; a short queue gets reviewed, a long one gets skimmed.
3. **Each pull request is rebuilt on upstream `main`**, not cherry-picked blind from `buwiz`. Our commits were
   written against 0.6.1 and the surrounding code has moved.
4. **Split by problem, not by commit.** A theme is usually one pull request; a theme that fixes two things is two.
5. **The fork line is not touched here.** Landing a change upstream does not remove it from `buwiz`. The ledger
   records it, and whoever next rebuilds the fork line on an upstream release drops what that release contains.
6. **gpui-agent is never mentioned** in an upstream title, description or comment, and no pull request exists to
   serve it. A change goes upstream because it is right for NSH.
7. **Doors come later, as their own track** (below).

## The work, per candidate

1. **Is it still needed?** On current upstream `main`, reproduce the bug or show the missing behavior. If upstream
   already has it, mark the theme "already upstream" in the ledger with the upstream commit, and move on.
2. **Branch from upstream `main`** in `hexuria/nsh`: `up/<area>-<short-name>`.
3. **Port the change** to today's code, with a test at the layer that owns the behavior. Trace the fault to its
   origin; do not send a guard or a retry in place of the cause.
4. **Run upstream's checks**, including the platform clippy run on that platform's machine, and run the related
   `examples/gallery` demo there. Take before and after screenshots for anything visible.
5. **Open the pull request** from `hexuria:up/...` to `longbridge:main`, with the template filled in.
6. **Answer review.** Upstream's first review is often automated ("Reviewed by ChatGPT on behalf of ..."). Fix what
   is right, explain what is not, keep the diff small.
7. **Write the ledger row**, and update it when the pull request is merged or closed.

## The ledger

`handover/upstream-prs/LEDGER.md` on this branch, one row per theme, kept current:

```
| Theme | State | Issue | PR | Our commits it replaces | Upstream commit | Notes |
```

States: `not started`, `already upstream`, `issue open`, `agreed`, `declined`, `PR open`, `merged`, `closed`,
`not for upstream`. This file is how the owner, and the session that next rebuilds the fork, know what to drop.

## Doors

The other handover produces `SOCKET.md` on branch `buwiz-socket`: the list of small public items our own crates
need from NSH. Each is new public API, so each goes through the new-API path: an issue first.

The case to make is upstream's own rule: a feature belongs in NSH only when an nshm cannot provide it. A door is
the smallest core change that lets something live in an nshm instead of in NSH. Make that case in general terms
(testing tools, automation, accessibility inspection built outside NSH), one door or one small group per issue.

Start this track only once `SOCKET.md` exists and its doors are proven on all platforms.

## Done when

- [ ] Every theme in `data/themes.tsv` has a ledger row in a final state, or an open issue or pull request.
- [ ] Each B theme is merged, already upstream, or closed with the maintainer's reason recorded.
- [ ] Each N theme has an issue with an answer, and a pull request if the answer was yes.
- [ ] The ledger says, per theme, which of our commits a future catch-up can drop.
