# iOS development

The UIKit backend lives in `crates/nsh-platform-ios`; `nsh-ios-host` is the development host and `nsh-ios-app` the
packaged runner. See [commands](commands.md) for simulator and Catalyst builds, [pitfalls](pitfalls.md#ios-uikit-and-mac-catalyst)
for UIKit constraints, and the [iOS guide](../../website/docs/ios.md) for running and packaging apps.

## GPUI-agent simulator attach

`nsh-ios-host` enables `nsh-platform-ios/dev-agent` only for the simulator. The feature pulls in `nsh-gpui-agent`;
ordinary runners and embedding builds do not enable it. `DevSession` in `src/dev.rs` owns one shared-adapter worker,
with `PlatformKind::Mobile`, per opted-in session. Its model is installed on `view::Inner` before bundles arrive,
so new surfaces and hot reloads keep the same model. `agent::IosNative` dispatches native input and screenshots on
the main queue. Its liveness flag becomes false when the session drops, before the worker requests shutdown;
dropping the view also drops its session. No main-thread join waits for pending native work.

`nsh run examples/gallery --target ios` with `GPUI_AGENT=1` and `GPUI_AGENT_TOKEN` writes the token to
`Library/Caches/gpui-agent-token` in the installed app's data container, found with `simctl get_app_container ... data`.
The file has mode `0600`; the host reads it once and deletes it before binding. Launch failure and session teardown
also remove it. `SIMCTL_CHILD_GPUI_AGENT` and `SIMCTL_CHILD_GPUI_AGENT_ADDR` carry only the opt-in and address;
`SIMCTL_CHILD_GPUI_AGENT_TOKEN` is explicitly removed. No token is passed in process arguments or the app environment.

The default address is `127.0.0.1:17421`, shared with the Mac; custom addresses must also be loopback. Screenshot
paths are relative `.png` names confined to `Library/Caches/gpui-agent`; use `gpui-agent screenshot --inline --out ...`
to save an image on the Mac. Release builds refuse attach, and physical-device and Catalyst hosts do not enable it.
The [tooling guide](tooling.md#ios-simulator-gpui-agent-development) has launch commands.
