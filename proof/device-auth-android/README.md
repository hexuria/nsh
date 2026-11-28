driver: adb emu screenrecord + framework biometric prompts; app examples/device-auth copies; final kit nsh commit 021543a64216817e5b408204458e8bc089814391; taffy 0.15.0

Android biometric device-authentication emulator proof on the reverted tree: Taffy is 0.15.0; no Taffy 0.16 code is present. The first matrix ran at dd3366e12004e73faacc7612f297e6feec62a9fc. The options-object rerun rows are under `options-367594128/`; those packaged apps were built at 3675941284d2c8cf36c12abfee65dd98e369ce64, and the final head adds only a clippy-driven construction refactor in the same Rust call path. The credential example copy guards against re-entering a request already in flight when the activity lifecycle resumes; the platform behavior under test is unchanged.

| API | exact expected rows |
|---|---|
| 26 | 1/1 |
| 28 | 5/5 |
| 29 | 5/6 |
| 35 | 7/7 |

API 29 `cred-pin` is a measured failure, not a crash: the correct PIN is entered, but the request never settles. Logcat shows the API 29 system service dropping the authentication session while `ConfirmDeviceCredential` moves from its inner biometric prompt to the PIN (`onCDCASuccess null!`). Verdicts are parsed only from the most recently started app process because the emulator log buffers can retain earlier rows.

Per-API tables are in `apiNN-README.md`; recordings, prompt screenshots and logcat output are under `apiNN/<row>/`.

## Options-object rerun

The rerun rows for commit 021543a64216817e5b408204458e8bc089814391 are under `options-367594128/`: API 29 6/6, API 28 regression 1/1, and API 35 3/3 exact passes. Taffy remains 0.15.0.
