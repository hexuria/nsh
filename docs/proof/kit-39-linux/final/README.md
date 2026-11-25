# kit #39 (fix/otp-input-burst) Linux proof at 6149250d

Machine: buwiz-forms-linux-2, Ubuntu, Xvfb-free KDE desktop on :0 (2560x1920), docker image
buwiz-nsh-2404, toolchain cargo (RUSTC_WRAPPER empty), one build folder per commit:
target-kit39final (6149250d, clone ~/repos/nsh-kit39-final) and target-kit39pre (62e436ff,
clone ~/repos/nsh-kit39-pre). Tell: the binaries behave differently exactly where the patch
differs (step 2 clipboard), and each clone was built only in its own folder.

## Results table

| step | commit | command | verdict | counts / notes |
|---|---|---|---|---|
| 1 linux build | 6149250d | cargo clippy -p nsh-platform-linux -p nsh-linux-host -p nsh-linux-app --all-targets | PASS | 0 warnings, 0 errors (clippy-linux.log) |
| 2 otp_input tests | 6149250d | cargo test -p nsh-base --test base otp_input | PASS | 29 passed, 0 failed; 11 new incl. the arrow ones |
| 2 nsh-testing | 6149250d | cargo test -p nsh-testing | PASS | 114 passed, 0 failed |
| 2 command encoding | 6149250d | cargo test -p nsh-core / -p nsh-js | PASS* | core 1715 passed, 1 failed — keyed_list_cell_whose_row_goes_can_read_first, a keyed-list component test UNRELATED to otp/encoding, fails consistently on rerun; js 243 passed 0 failed |
| 2 engine otp | 6149250d | cargo test -p nsh-engine --test engine otp | PASS | 1 passed (of 466 filtered) |
| 3 revert experiment | otp_input.rs at 62e436ff content + head harness | cargo test -p nsh-base --test base otp_input x3 | 4 FAIL x3 | the same four every run: a_caret_inside_the_text_is_the_users_own, a_bare_backspace_after_arrows_removes_the_digit_before_the_caret, an_arrowed_selection_cuts_exactly_it, typing_over_an_arrowed_selection_replaces_exactly_it (k39-revert-run*.log) |
| 4 linux selection write | diff | git diff 62e436ff 6149250d -- crates/nsh-platform-linux | unchanged | empty diff: nothing in the Linux platform crate differs |
| 5 GUI step 1 | 6149250d | type 123456 at 10ms | PASS | boxes 1 2 3 4 5 6 (shots-head/step1) |
| 5 GUI step 2 | 6149250d | 123, Left, Shift+Left, Ctrl+X | PASS | boxes 1 3, clipboard "2" (step2, step2-clipboard-verify) |
| 5 GUI step 3 | 6149250d | 123, Home, Shift+Right, Delete | PASS | boxes 2 3 (step3) |
| 5 GUI step 4 | 6149250d | 123, Ctrl+A, Ctrl+X | PASS | empty (step4) |
| 5 GUI step 5 | 6149250d | Ctrl+V of 987654 | PASS | boxes 9 8 7 6 5 4 (step5) |
| 5 GUI step 1 | 62e436ff | same | PASS | boxes 1 2 3 4 5 6 (shots-pre/step1) |
| 5 GUI step 2 | 62e436ff | same | **FAIL (the bug)** | boxes 1 2, clipboard "3": the cut took the LAST digit, not the selected one |
| 5 GUI step 3 | 62e436ff | same | **FAIL (the bug)** | boxes still 1 2 3: forward Delete of the arrow-made selection removed nothing |
| 5 GUI step 4 | 62e436ff | same | PASS | empty |
| 5 GUI step 5 | 62e436ff | same | PASS | boxes 9 8 7 6 5 4 |

GUI method: real GTK window (cargo run -- OtpInput, gallery built for the Linux host,
window title "Gallery"), real key events through xdotool --clearmodifiers, clipboard read
with xclip, screenshots with import. No key combination did nothing at the head; at
62e436ff the forward-Delete combination visibly did nothing (step 3). The "That code is
not right" caption in some shots is the demo's own wrong-code shake (the demo's correct
code is 246810), not a fault.

## Machine differences / environment notes
- xtask's launcher expects target/debug/nsh relative to the checkout; with a redirected
  CARGO_TARGET_DIR a `target` symlink to the build folder is needed or the launch fails
  ("error launch target/debug/nsh"). Same for nsh-linux-host, which must be cargo-built
  separately (clippy does not emit the binary).
- The 1m05s "Finished" line cargo prints for the clippy run is its own timing; the full
  cold dependency build is longer wall-clock.
- mid-run the machine's disk filled (old 64 GB of prior build folders; since removed).
  Two test runs that failed with "No space left on device" during that window were
  re-run clean after; no other effect.
