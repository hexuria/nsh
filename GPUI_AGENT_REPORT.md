# GPUI-agent through NSH's test agent

## Scope

Prototype for Linux GTK 4 and Android, using `gpui-agent = "=0.1.0"` (wire protocol 2).
The macOS implementation was read as a behavior reference; it is not replaced by this branch.
The proposed `rox/drafts/nsh-issue-agent-attach.md` was absent, so this implementation follows the four steps supplied in the task.

## Adapter design (12 lines)

1. `nsh-testing` projects the mounted scene into semantic nodes, including offscreen nodes.
2. Semantic scene queries carry roles, values, test IDs, state, ancestry, and keybindings.
3. Strict actionability is opt-in; ordinary `Agent::new` keeps its existing behavior.
4. Strict actions reject disabled nodes and intersect native bounds with the viewport and clipping ancestors.
5. Offscreen targets are revealed through the existing scroll-and-settle machinery before failing.
6. Native hit testing must resolve to the target or its descendant; foreign hits return `occluded_by`.
7. `nsh-gpui-agent` translates the protocol once, with deterministic collision-safe semantic IDs.
8. GTK picking and Android Java/JNI supply native geometry, input, focus, and PNG capture.
9. Each attached host maintains its own model, independent of an ordinary `nsh test` run.
10. Development workers require a nonempty token and a loopback bind address; Android additionally checks debuggability.
11. Screenshot helpers confine output; upstream authorization gates dangerous keybindings.
12. Lifecycle invalidation fails closed, and shutdown does not join the worker on the UI thread.

## Environment and launch

The host lacked GTK 4.14 development packages. Linux builds and Xvfb runs used an Ubuntu 24.04 Docker container with GTK 4.14.5; the repository was mounted at `/work`, with a separate target cache at `/target`.
Android used an API 35 Google APIs x86_64 emulator accelerated by KVM, SDK build tools 35.0.0 for the final isolated fixture packaging, and NDK 27.2.12479018. Installing `libclang-dev` resolved the first Android bindgen failure.

The CLI was built from `hexuria/gpui-agent`:

```sh
CARGO_TARGET_DIR=/home/ubuntu/.cache/gpui-agent-target CARGO_BUILD_JOBS=4 \
  cargo build --manifest-path /home/ubuntu/repos/gpui-agent/Cargo.toml \
  -p gpui-agent-cli --bin gpui-agent --locked
# Finished dev profile successfully
```

Sanitized Linux launch, inside the GTK container:

```sh
GPUI_AGENT=1 GPUI_AGENT_ADDR=127.0.0.1:17421 GPUI_AGENT_TOKEN=<redacted> \
  xvfb-run -a /target/debug/nsh run --target linux examples/gallery
```

The randomly generated token was retained privately and reused by the CLI, never written to the repository or this report. For a new run, export `GPUI_AGENT_TOKEN="$(openssl rand -hex 32)"` once in the shell shared by the launch and CLI commands. Every CLI command below uses the matching address and token.

Android forwarding and sanitized launch:

```sh
/home/ubuntu/Android/Sdk/platform-tools/adb forward tcp:17421 tcp:17421
adb forward --list
# emulator-5554 tcp:17421 tcp:17421

GPUI_AGENT=1 GPUI_AGENT_ADDR=127.0.0.1:17421 GPUI_AGENT_TOKEN=<redacted> \
  NSH_RUNTIME=/home/ubuntu/.cache/gpui-agent-runtime \
  /home/ubuntu/repos/nsh/target/debug/nsh run --target android \
  /home/ubuntu/.cache/gpui-agent-android-gallery
```

That app directory is a copy of `examples/gallery`. The default all-ABI development APK initially failed with `INSTALL_FAILED_INSUFFICIENT_STORAGE` (2.4 GB APK versus 2.2 GB free). Live runs used a user-owned x86_64-only runtime copy with stripped debug symbols (128 MB native library), without changing the repository runtime artifacts. The negative fixture used a second gallery copy with `tests/fixtures/agent-negative.tsx` as its entry.

## Linux live results

`gpui-agent hello` reported protocol 2, `os: linux`, `ready: true`, `auth: required`. `snapshot`, `click`, `type`, `key`, `invoke press`, `invoke scroll`, `invoke hover`, `invoke drag`, `invoke viewport`, `wait`, and `screenshot` returned responses. Snapshots were captured after each action. IDs below are ephemeral native scene IDs from the recorded run, not reusable selectors.

Trimmed CLI observations (each command has the `gpui-agent` prefix):

```text
click n1760
  success; checkbox label changed from "3 of 4" to "4 of 4"
type agent-input linux-check
  success; input value and echoed label became "linux-check"
invoke hover --arg target=cover
  success; snapshot unchanged (original fixture had no hover feedback)
invoke drag --arg target=reveal-scroll --arg dx=0 --arg dy=40
  success; snapshot unchanged at the scroll limit
invoke viewport --arg width=320 --arg height=640
  success; following snapshot root 320 x 640
invoke viewport --arg width=390 --arg height=844
  success; following snapshot root 390 x 844
wait --timeout-ms 5000
  ready=true
```

The first Backspace check cleared the entire focused input after `linux-check`, exposing repeated GTK focus acquisition; the final verification section records the subsequent fix/check.
The gallery also exercised scroll and the phone preset. Screenshot output rejected absolute paths and accepted a confined relative PNG name; the original gallery PNG was 57 KB. No PNG re-encoding is performed by the adapter.

### Native actionability rejections

```text
click covered-target
  exit 1: occluded_by: 56
click disabled-target
  exit 1: node 62 is disabled
invoke scroll --arg target=reveal-scroll --arg dy=500
  exit 0; scroll-clip y=173, unreachable-target y=673, visible=false
click unreachable-target
  exit 1: node 74 is offscreen
```

Following snapshots retained `presses:0`, an empty input, and `scrolls:2`; the unreachable node remained in the semantic tree with `visible:false`. The ordinary fixture test also passed: `nsh test examples/gallery/tests/agent-negative.test.tsx` (1 passed, 0 failed).

## Android live results

`hello` reported protocol 2, `platform: mobile`, `os: android`, `ready: true`, `auth: required`. Initial root bounds were approximately 411.43 x 914.29 dp.

```text
gpui-agent invoke scroll --arg target=n32 --arg dy=600
  success; Form target n208 moved from y=771.43 to y=229.71
gpui-agent click n208
  success; following snapshot showed the Form page
gpui-agent type n878 android-check
  success; following snapshot n878.value="android-check"
gpui-agent key n878 Backspace
  success; following snapshot n878.value="android-chec"
gpui-agent invoke hover --arg target=n878
  success; snapshot value unchanged; this control exposes no hover feedback
gpui-agent invoke drag --arg target=n788 --arg dx=0 --arg dy=-250
  success; Notes node n1400 moved from y=2167.24 to y=174.48
gpui-agent invoke viewport --arg width=320 --arg height=640
  success; response and following snapshot root 320 x 640
gpui-agent invoke viewport --arg width=411 --arg height=914
  success; response and following snapshot root 411.05 x 913.90 (pixel rounding)
gpui-agent wait --timeout-ms 5000
  ready=true; root size unchanged
```

Screenshots were returned under the app cache and copied out with `adb exec-out run-as`; the gallery and negative captures were native 1080 x 2400 RGBA PNGs. Android viewport changes resize the development content view, not the emulator display. Requests beyond the physical window cannot honestly emulate a larger screen and must not be interpreted as display resizing.

### Native actionability rejections

```text
gpui-agent click covered-target
  exit 1: occluded_by: 18
gpui-agent click disabled-target
  exit 1: node 24 is disabled
gpui-agent invoke scroll --arg target=reveal-scroll --arg dy=500
  exit 0; scrolls:0 -> scrolls:1; unreachable-target y=673.14, visible=false
gpui-agent click unreachable-target
  exit 1: node 36 is offscreen
```

Following snapshots retained `presses:0` and an empty input; the unreachable node remained `visible:false`. Rejected clicks did not activate the covered/disabled/unreachable controls.

## Initial implementation checks (pre-compatibility correction)

These results record the initial implementation before compatibility-fix commit `746960da12eab1c48324e618c79a63b7c11edda6` and merge commit `fabbf035ee3e5c2cd03aad9026a265aec6afab43`. The post-merge checks are recorded below.

```text
cargo test -p nsh-dev
  7 unit tests and 6 integration tests passed
cargo test -p nsh-testing --features agent --lib
  33 passed
cargo test -p nsh-gpui-agent --features dev-agent
  7 passed
cargo clippy -p nsh-gpui-agent --features dev-agent --all-targets -- -D warnings
  Finished dev profile
cargo clippy -p nsh-testing --features agent --all-targets -- -D warnings
  Finished dev profile
cargo clippy -p nsh-cli --all-targets -- -D warnings
  Finished dev profile
cargo check -p nsh-testing --no-default-features
  Finished dev profile
cargo +nightly fmt --all
  passed
cargo +nightly fmt --all --check
  passed
cargo xtask android --dev --abi x86_64
  built libnsh.so for x86_64 (10.6s); compiled Java runtime (4.5s)
  output: target/android/debug-dev
RUSTFLAGS='-A clippy::missing_const_for_thread_local' cargo xtask clippy-android
  all-features pass: Finished dev profile (1.95s)
  no-default-features pass: Finished dev profile (0.80s)
```

Android commands used `ANDROID_HOME` and `ANDROID_SDK_ROOT` pointing to `/home/ubuntu/Android/Sdk`, and `ANDROID_NDK_HOME` to its `ndk/27.2.12479018` directory. The unmodified clippy command failed on `nsh-core/src/element/engine_context.rs:67`, `element/renderer.rs:72`, and `time.rs:54`; the single lint allowance above leaves other warning gates enabled.

Final GTK commands and trimmed output:

```sh
docker exec -w /work -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=4 nsh-agent-gtk24 \
  cargo clippy -p nsh-platform-linux -p nsh-linux-host --no-default-features \
  --features nsh-platform-linux/dev --all-targets -- -D warnings
# Finished dev profile (2.94s)
docker exec -w /work -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=4 nsh-agent-gtk24 \
  cargo build -p nsh-linux-host --no-default-features
# Finished dev profile (14.58s)
docker exec -w /work -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=4 nsh-agent-gtk24 /target/debug/nsh test \
  examples/gallery/tests/agent-negative.test.tsx
# 1 test passed, 0 failed
```

The direct host-shell fixture test was blocked by the existing root-owned mode-0700 `examples/gallery/.nsh` directory; running the same test in the GTK container succeeded without changing permissions.

Final Linux regression on a separate Xvfb fixture at port 17423:

```text
gpui-agent type agent-input linux-check
  ok=true; snapshot: agent-input.value="linux-check", presses:0
gpui-agent key agent-input Backspace
  ok=true; snapshot: agent-input.value="linux-chec", presses:0
gpui-agent invoke hover --arg target=cover
  ok=true; snapshot: hoverIn:1 hoverOut:0
gpui-agent invoke hover --arg target=agent-input
  ok=true; snapshot: hoverIn:1 hoverOut:1
```

The focused tests cover unavailable scenes, worker configuration, semantic visibility and ID collisions, viewport validation, dangerous-keybinding authorization, clipping and descendant hits, and rejected actions without native side effects. Linux focus now preserves an already-focused descendant rather than selecting the whole input again.

### Latest Android hover verification

The final x86_64 `target/android/debug-dev` runtime was installed into an isolated debuggable fixture host, `com.longbridge.nsh.agentfixturehost`, to avoid replacing the original gallery. It loaded the committed fixture through a separate development server. No repository source or runtime build was changed for this check; only a cache copy was stripped and packaged.

```sh
/home/ubuntu/repos/nsh/target/debug/nsh dev --server-only --port 8793 /home/ubuntu/.cache/gpui-agent-android-hover-fixture
# built App.tsx: 9.4 KB JS, 25.9 KB bytecode
adb -s emulator-5554 reverse tcp:8793 tcp:8793
adb -s emulator-5554 forward tcp:17427 tcp:17427
TOKEN=$(< /home/ubuntu/.cache/gpui-agent-android.token)
adb -s emulator-5554 shell am start -W -n com.longbridge.nsh.agentfixturehost/com.longbridge.nsh.DevActivity \
  --es url ws://127.0.0.1:8793/ws --es GPUI_AGENT 1 \
  --es GPUI_AGENT_ADDR 127.0.0.1:17427 --es GPUI_AGENT_TOKEN "$TOKEN"
# Status: ok
export GPUI_AGENT_ADDR=127.0.0.1:17427 GPUI_AGENT_TOKEN="$TOKEN"
gpui-agent hello
# ready=true, os=android, auth=required
gpui-agent snapshot
# agent-hover-state.name="hoverIn:0 hoverOut:0"
gpui-agent invoke hover --arg target=cover
# ok=true
gpui-agent snapshot
# agent-hover-state.name="hoverIn:1 hoverOut:0"
gpui-agent invoke hover --arg target=agent-input
# ok=true
gpui-agent snapshot
# agent-hover-state.name="hoverIn:1 hoverOut:1"
adb -s emulator-5554 shell input keyevent 4
# restored original gallery without restarting its process
adb -s emulator-5554 shell am force-stop com.longbridge.nsh.agentfixturehost
adb -s emulator-5554 forward --remove tcp:17427
adb -s emulator-5554 reverse --remove tcp:8793
```

Both galleries were left running side by side on the visible Desktop, through TigerVNC for the GTK/Xvfb app and scrcpy for Android. These are live interactive views, not screenshot slides.

### Hosted CI status

The fork pull request is https://github.com/hexuria/nsh/pull/2. After the implementation push, PR checks and commit checks reported **0 passed, 0 failed, 0 pending, 0 skipped**. GitHub Actions queries for the branch, pull-request event, and implementation head each returned `total_count: 0`. There were no job logs to inspect or failures to repair. Hosted CI is **not verified**; repository Actions enablement should be checked by the owner before relying on CI. The local gates and live acceptance results above are the evidence for this prototype.

## Post-merge compatibility correction checks

The review fixes serialize Android model reset with batch application after the old engine joins; catalog installed application-menu shortcuts independently of popup menus; map canvas regions and linked text spans to precise native-owner geometry; and include the adapter crate in Linux and Rust-app workflow path filters. Reload acceptance and reset ordering change, not protocol transitions, settle deadlines, or retries.

Before the merge, two full bundle reloads were exercised in the isolated Android fixture host. After each `gpui-agent wait --timeout-ms 30000`, snapshots contained all nine replacement IDs and none of the prior IDs, and the replacement input was empty. The fixture process stayed alive across both reloads. The original gallery was preserved and restored afterward. This is live functional evidence, not an exhaustive concurrency proof.

Commit `746960da12eab1c48324e618c79a63b7c11edda6` restores ordinary `Agent::find` to the reviewed `deb15fab` behavior: selectors use `Pick`, `CanvasNode`, `Span`, and the existing match queries without requiring semantic-tree support. The correction removes the `Scene::canvas_target` trait method and `SceneQuery::CanvasTarget` wire query; the private `Screen::canvas_target` helper remains behind `canvas_node`. `SceneQuery::Bounds` again reports native bounds, while `Agent::native_bounds` retains synthetic target mapping for adapter snapshots. Strict node-ID actions retain synthetic geometry, clipping, and native-hit checks inside `actionable_point`. The new remote-scene regression rejects `SemanticTree` queries and exercises ordinary canvas find/press.

The compatibility fix was merged with `origin/main` at `1849776f68ee41d73c08970fe530cf952451af13` by merge commit `fabbf035ee3e5c2cd03aad9026a265aec6afab43`; the merge completed without conflicts.

Post-merge focused gates:

```text
cargo test -p nsh-testing --features agent --lib
  36 passed, 0 failed, 0 ignored
cargo test -p nsh-gpui-agent --features dev-agent
  7 passed, 0 failed; 0 doc tests
cargo test -p nsh-dev
  7 unit tests and 6 integration tests passed; 0 doc tests
cargo clippy -p nsh-testing --features agent --all-targets -- -D warnings
  Finished dev profile successfully
cargo clippy -p nsh-gpui-agent --features dev-agent --all-targets -- -D warnings
  Finished dev profile successfully
cargo check -p nsh-testing --no-default-features
  Finished dev profile successfully
cargo check -p nsh-testing --features agent --tests
  Finished dev profile successfully; test code compiled, tests not run by this command
cargo +nightly fmt --all --check
  passed after the merge; not rerun for the report-only edit
python3 .github/ci/paths.py
  exited successfully after the merge; not rerun for the report-only edit
git diff --check
  passed after the merge
```

GTK 4.14 container checks, with `CARGO_TARGET_DIR=/target` and `CARGO_BUILD_JOBS=4`:

```text
docker exec -w /work -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=4 nsh-agent-gtk24 \
  cargo clippy -p nsh-platform-linux -p nsh-linux-host --no-default-features \
  --features nsh-platform-linux/dev --all-targets -- -D warnings
  Finished dev profile successfully
docker exec -w /work -e PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=4 nsh-agent-gtk24 \
  cargo build -p nsh-linux-host --no-default-features
  Finished dev profile successfully
```

Android x86_64 development build and clippy, using SDK `/home/ubuntu/Android/Sdk` and NDK `27.2.12479018`:

```text
ANDROID_HOME=/home/ubuntu/Android/Sdk ANDROID_SDK_ROOT=/home/ubuntu/Android/Sdk \
ANDROID_NDK_HOME=/home/ubuntu/Android/Sdk/ndk/27.2.12479018 \
CC_x86_64_linux_android=/home/ubuntu/Android/Sdk/ndk/27.2.12479018/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android24-clang \
AR_x86_64_linux_android=/home/ubuntu/Android/Sdk/ndk/27.2.12479018/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar \
CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=/home/ubuntu/Android/Sdk/ndk/27.2.12479018/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android24-clang \
BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android=--sysroot=/home/ubuntu/Android/Sdk/ndk/27.2.12479018/toolchains/llvm/prebuilt/linux-x86_64/sysroot \
cargo xtask android --dev --abi x86_64
  built libnsh.so for x86_64 and compiled the Java runtime; output target/android/debug-dev

ANDROID_HOME=/home/ubuntu/Android/Sdk ANDROID_SDK_ROOT=/home/ubuntu/Android/Sdk \
ANDROID_NDK_HOME=/home/ubuntu/Android/Sdk/ndk/27.2.12479018 \
RUSTFLAGS='-A clippy::missing_const_for_thread_local' cargo xtask clippy-android
  all-features pass; no-default-features pass
```

The Android clippy allowance is limited to the existing `nsh-core` `missing_const_for_thread_local` warnings in `element/engine_context.rs:67`, `element/renderer.rs:72`, and `time.rs:54`; no unrelated core lint changes were made. Live two-reload acceptance was not repeated: its evidence remains pre-merge. The existing galleries, VNC, emulator, and scrcpy processes were left running.

The outstanding `Default` convention question remains unresolved by design: do not invent `Default` implementations for `NshAgentHost` or `WorkerHandle`, since valid construction requires real resources. This still needs an owner decision or an explicit convention exception.

## Button-label press owner correction

This follow-up starts from the reviewed feature-branch head `9cc82ae5b41684582247708d341ddffcda00ffc1`; the preceding report snapshot was `80a59fe7afa831fe96dfc0393b0326663a3e9888`. Commit `3d8d9f8071cfa4ddd6fd67c5e43fa800710b2eaf` fixes strict `press_node` owner selection: ordinary native views use their resolved pressable ancestor for hit verification, while synthetic canvas/text regions retain their precise native owner. This lets a button-label press accept a native hit on the button or its descendant, but still rejects a foreign hit before dispatching a press. The regression `button_label_press_accepts_its_parent_or_descendant_but_not_foreign_hits` covers parent, descendant, and foreign hits, including no press side effect on rejection. No protocol or public API changed.

Latest shared checks for this correction:

```text
cargo test -p nsh-testing --features agent --lib button_label_press_accepts_its_parent_or_descendant_but_not_foreign_hits
  1 passed, 0 failed, 36 filtered out
cargo test -p nsh-testing --features agent --lib
  37 passed, 0 failed, 0 ignored
cargo test -p nsh-gpui-agent --features dev-agent
  7 passed, 0 failed; 0 doc tests
cargo clippy -p nsh-testing --features agent --all-targets -- -D warnings
  Finished dev profile successfully
cargo clippy -p nsh-gpui-agent --features dev-agent --all-targets -- -D warnings
  Finished dev profile successfully
cargo +nightly fmt --all
  passed
cargo +nightly fmt --all --check
  passed
git diff --check
  passed
```

The GTK and Android builds/clippy plus live runtime evidence above cover the pre-correction revision at `9cc82ae5`; they were not rerun for this shared `nsh-testing` owner-selection fix. No runtime or app processes were changed.

## Limitations and unverified behavior

- The raw Android clippy command encounters pre-existing `nsh-core` `missing_const_for_thread_local` warnings. The final checks above distinguish that baseline failure from the lint-allowed result; no unrelated core lint rewrite is included.
- Android and Linux hover dispatch is implemented. The original gallery checks did not demonstrate visible hover feedback; the final fixture checks above distinguish dispatch success from app callback evidence.
- Linux drag at the fixture's scroll limit produced no visible change; Android gallery drag did move content.
- Native application menus are exposed where supported; dangerous bindings use upstream confirmation. Real destructive menu commands were not fired against the gallery.
- Shutdown returns an explicit unsupported error for an attached app; it does not terminate an embedding host.
- The adapter intentionally does not claim a monotonic protocol revision.
- Native hit testing proves occlusion within the attached app's view hierarchy, not by unrelated operating-system windows.
- macOS, iOS, Windows, full workspace tests, and cross-platform packaging are not claimed as verified by these focused Linux/Android runs.

## Commits

| Commit | Area and purpose |
| --- | --- |
| `c3ef5ab5` | Core: clone mount state for independent agent hooks. |
| `3e04b1c1` | Dev protocol: add semantic-tree and keybinding scene queries. |
| `337f4086` | Testing: semantic projection and opt-in native actionability safeguards. |
| `8223b927` | Adapter: shared GPUI-agent translation and development worker. |
| `b788cf35` | Linux: GTK attach, native hooks, focus preservation, and docs. |
| `ecd1cec1` | Android: development attach, JNI/Java native hooks, lifecycle, and xtask feature wiring. |
| `e7d98425` | CLI: pass Android opt-in/token extras and redact launch failures. |
| `e196ca4d` | Fixtures: deterministic safety/hover fixture and Android launch guide. |
| `73c6fb59` | Android: serialize scene reset and batch application after stopping the previous engine. |
| `fc03d19a` | Testing: catalog installed application-menu shortcuts independently of popup lifecycle. |
| `ccee970f` | Dev: describe installed-menu shortcut metadata. |
| `02188ac2` | Testing: resolve canvas and linked-span targets through precise native-owner geometry. |
| `d5fdcfe5` | CI: include adapter changes in Linux and Rust-app workflow filters. |
| `746960da12eab1c48324e618c79a63b7c11edda6` | Testing: restore legacy ordinary selector queries and add a semantic-tree-independent find/press regression. |
| `fabbf035ee3e5c2cd03aad9026a265aec6afab43` | Merge: merge updated `origin/main` without rewriting history. |
| `80a59fe7afa831fe96dfc0393b0326663a3e9888` | Report: prior post-merge validation snapshot. |
| `3d8d9f8071cfa4ddd6fd67c5e43fa800710b2eaf` | Testing: preserve the pressable ancestor for strict native button-label presses. |
| This final report commit | Report: record the button-label correction and latest shared checks. |

Earlier report snapshot: `deb15fabc110e01e52fc8c11de7cb393dc57fd1d`; subsequent report snapshot: `80a59fe7afa831fe96dfc0393b0326663a3e9888`; this follow-up began at branch head `9cc82ae5b41684582247708d341ddffcda00ffc1`. Final report marker: this separate report-only commit, made last before pushing the feature branch. The report commit records the latest shared validation; no additional implementation changes are included in it.

The feature branch is `devin/gpui-agent-adapter`; it includes `origin/main` at `1849776f68ee41d73c08970fe530cf952451af13`. Only this feature branch was pushed. The report is committed last so it can be dropped before proposing the implementation upstream.

## What Windows needs

Enable the shared development adapter in the Windows development host; maintain its independent applied-batch/error/frame model and invalidate it on teardown. Implement `Native::hit_at` using the existing Win32/Direct2D input tree, returning mounted NSH IDs in the same typed coordinate space as bounds and viewport. Supply native focus/key validation, hover/drag, PNG screenshots, viewport sizing, and real menu bindings. Keep release runners free of the feature and retain loopback/token enforcement. Then run the same gallery and negative fixture through the CLI on Windows; no protocol translation fork is needed.
