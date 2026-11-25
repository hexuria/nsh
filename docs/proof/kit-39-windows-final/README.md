# Windows proof for hexuria/nsh PR #39 (fix/otp-input-burst) at 6149250d

Machine buwiz-forms-win-3: Windows Server 2022 Standard build 20348, UAC off, 1920x1080.
Gallery run: `cargo run -- otpinput` in a fresh clone at 6149250d0d650aad1c76ea44f4eb2c26478d08b8,
real key events into the real Win32 window (no automation API; physical-level keystrokes via the
machine's input). Screenshots are full-screen; the NSH gallery window is the left window.

- `01-burst-123456.png` — typed 123456 as a fast burst: boxes show 1 2 3 4 5 6 in order, Last event "Code complete: 123456". PASS.
- `02-left-shiftleft-cut-13.png` — clear, type 123, Left once, Shift+Left, Ctrl+X: boxes show 1 3 (caret in the third box), Last event "Code: 13"; the clipboard held exactly "2" (read with Get-Clipboard). PASS.
- `03-home-shiftright-delete-23.png` — clear, type 123, Home, Shift+Right, Delete: boxes show 2 3, Last event "Code: 23". PASS.
- `04-ctrla-ctrlx-empty.png` — clear, type 123, Ctrl+A, Ctrl+X: all boxes empty, Last event "Code: empty". PASS.
- `05-paste-987654.png` — clipboard set to 987654, Ctrl+V: boxes show 9 8 7 6 5 4 in order, Last event "Code complete: 987654". PASS.

No key combination was dead in this window; every step behaved as the task described.

Note on environment: the mandated dedicated CARGO_TARGET_DIR means xtask/nsh CLI do not find
nsh.exe / nsh-windows-host.exe at the repo-relative target/debug path they expect; the two
already-built binaries were copied there unchanged to launch the demo. Nothing was rebuilt or
patched for this; it is a lookup-path consequence of the custom target dir, not a Windows fault.
