# Ledger: what happened to each of our non-agent changes

One row per theme of `data/themes.tsv`. Keep it current: it is how the owner, and whoever next rebuilds the fork
line on an upstream release, know what the fork can drop.

States: `not started`, `already upstream`, `issue open`, `agreed`, `declined`, `PR open`, `merged`, `closed`,
`not for upstream`, `ready, not sent`.

| Theme | State | Issue | PR | Our commits it replaces | Upstream commit | Notes |
|---|---|---|---|---|---|---|
| B01 | ready, not sent | | | 4a1a00a3a 90176289a | | Branch `up/core-folded-text-quota`, local commit `45c5619a0`; tested green. push to hexuria/nsh blocked (403); PR not opened |
| B02 | ready, not sent | | | da41e0df2 | | Branch `up/animation-leaf-completions`, local commit `21f7814eb`; tested green. push to hexuria/nsh blocked (403); PR not opened |
| B03 | not started | | | 86d7bd945 cc9398e08 30eb63d84 | | windows: resize by the client area, counting the menu bar; requires Windows machine |
| B04 | not started | | | a09f40be6 | | windows: a web view opens the file page its app loaded (macOS has it upstream, PR #1496); requires Windows machine |
| B05 | not started | | | 1b09f2403 | | windows: take the keyboard back from web views when a press lands outside them; requires Windows machine |
| B06 | not started | | | 3bd56a954 | | macos: report Up and Down from a text input |
| B07 | not started | | | abe8d514c | | ios: read the dev session from the held view state on reload |
| B08 | not started | | | 38bcc1126 5afbb7d19 | | js: retire replaced app-menu callbacks |
| B09 | not started | | | 88b940945 | | macos: draw views at the scale they are shown at |
| B10 | not started | | | b20ad1c63 | | macos: keep the lines canvases set their texts in |
| B11 | not started | | | ad469b46d bada9694e | | macos: draw content about to scroll into view ahead of time |
| B12 | not started | | | aae18fbe2 | | macos: pause an embedded app while it is unseen |
| B13 | not started | | | 407a4aedb 52721e04a | | macos: key order between a hosted app view and its host |
| B14 | not started | | | 0f9457cc9 d0d8748ce d9c34068d bc0146178 | | embedding: AppView::set_error_handler on macOS, Linux and Windows |
| B15 | not started | | | ad9c1f10e 9b41761b2 812c8fb71 d5ec46d06 ca1c2e31f 8bd3cade7 e0f7673ed 1261defae 8700bbbe6 304d031f8 1477b8e6b 1acb7e75a faac5563f df336ec87 e5127c6ed | | linux: export the application menu to KDE Plasma's Global Menu |
| N01 | not started | | | 95b5600c6 5ff6b7607 9134d90e8 5937660b8 dcb5753d8 c4d7f1ef3 3fd60ea7c | | desktop: say whether the menu is in a global menu bar and whether a tray shows (Tray.available and friends) |
| N02 | not started | | | 93377c571 0da4044aa | | core: ElementRef::item_offset |
| N03 | not started | | | a6a26ec54 2e47e8c83 | | core: React Native handler names (onChangeText and others) |
| N04 | not started | | | f7afafd8c 815d991d3 f0d2d183e | | keymaps: bind a menu's reserved keystroke |
| N05 | not started | | | 386091c6d | | macos: run_app opens its window without taking focus |
| N06 | not started | | | bd7e99a96 f1dc9b196 da670ec52 8e82b93ff df22f7a79 e4835556d | | engine and macos: the jank line and the performance monitor controls |
| X01 | not for upstream | | | 804ed65c6 | | Drop at the next catch-up, once upstream #1600 is confirmed to cover our case. |
| S01 | not for upstream | | | 5127e87c4 27b5327f5 eb9586500 19afe9626 574e89af0 76da5edca f7f01dfbb def385970 b6f3c3b04 4ae9673ac f13759c27 ac7e16d12 e4fc782b3 b12344b10 47feb966f 7a86ce0f9 | | Goes with the agent extraction (`feat/nsh-agent-socket`). |
| S02 | not for upstream | | | e6c1594d0 6e4000bb3 3ae8bbc57 809d598fc 9d1c64c15 5805be459 72380e4b8 | | Goes with the agent extraction (`feat/nsh-agent-socket`). |
| S03 | not for upstream | | | 6f20ea967 | | Goes with the agent extraction (`feat/nsh-agent-socket`). |
| H01 | not for upstream | | | dad16b3e2 21b2c1281 | | Ours only. |
