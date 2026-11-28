driver: adb emu screenrecord + framework biometric prompts; kit nsh commit 3675941284d2c8cf36c12abfee65dd98e369ce64; taffy 0.15.0

Options-object API proof. `deviceAuth` is called as `deviceAuth(reason, { allowDeviceCredential })` or with no options; Rust uses `DeviceAuthOptions`. API 28-29 credential-allowed prompts are biometric-only and Use PIN answers `no`; API 30+ offers the device credential.

| API | exact expected rows |
|---|---|
| 29 | 6/6 |
| 28 | 1/1 |
| 35 | 3/3 |

Rows here are the rerun rows requested for this head. API 26 and the other API 35 rows from the preceding proof stand because the Android DeviceAuth implementation did not change in this commit; the public JS/Rust call shape did.

Per-API tables are in `apiNN-README.md`; recordings, prompt screenshots and logcat output are under `apiNN/<row>/`.
