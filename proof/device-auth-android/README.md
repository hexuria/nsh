driver: adb emu screenrecord + framework biometric prompts; app examples/device-auth copies; kit nsh commit dd3366e12004e73faacc7612f297e6feec62a9fc; taffy 0.15.0

Android biometric device-authentication emulator proof on the reverted tree: Taffy is 0.15.0; no Taffy 0.16 code is present. The credential example copy guards against re-entering a request already in flight when the activity lifecycle resumes; the platform behavior under test is unchanged.

| API | exact expected rows |
|---|---|
| 26 | 1/1 |
| 28 | 5/5 |
| 29 | 5/6 |
| 35 | 7/7 |

API 29 `cred-pin` is a measured failure, not a crash: the correct PIN is entered, but the request never settles. Logcat shows the API 29 system service dropping the authentication session while `ConfirmDeviceCredential` moves from its inner biometric prompt to the PIN (`onCDCASuccess null!`). Verdicts are parsed only from the most recently started app process because the emulator log buffers can retain earlier rows.

Per-API tables are in `apiNN-README.md`; recordings, prompt screenshots and logcat output are under `apiNN/<row>/`.
