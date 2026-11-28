# Android device authentication proof

The fingerprint in this proof was **SIMULATED** with the Android emulator's
sensor command (`adb emu finger touch <id>`). Every result was read from the
example app's own log (`NSHDEVAUTH` in the JS message; Android logcat carries
it under the platform logger tag `nsh` as `js: NSHDEVAUTH ...`). The OEM
prompt (Samsung/Pixel) and real fingerprint sensors are **UNPROVEN**.

## Harness

- Example: `examples/device-auth`, calling `deviceAuthAvailable()` and
  `deviceAuth("Unlock the example", false)` at launch and on each resume.
- API 35 Google x86_64 emulator (`buwiz_api35`) for the enrolled rows.
- API 35 AOSP ATD x86_64 emulator (`deviceauth_atd35`) for the no-enrollment
  rows, because its fresh data partition has neither a fingerprint nor a
  device credential.
- The images exposed no working `cmd fingerprint`/`cmd biometric` enrolment
  command. Enrolment was therefore emulator-local: PIN `1111` was set, the
  Ranchu fingerprint HAL template (`sensor0.bin`) and framework fingerprint
  metadata (`settings_fingerprint_0.xml`) for simulated finger id `1` were
  written with root adb access, and the fingerprint service was restarted.
  No production service or real user data was involved.
- The framework BiometricPrompt itself has no timeout. NSH cancels an
  unanswered prompt after 30 seconds; the cancel rows use that path.
- Android `screencap` cannot capture BiometricPrompt because it is a secure
  window. Screenshots and recordings use the emulator console's
  `emu screenrecord` capture.
- Ranchu keeps a cumulative failure count. The harness restarted
  `android.hardware.biometrics.fingerprint-service.ranchu` before lockout
  rows so each row starts with zero failures.

## Rows

Each row was run three times. `verdict.txt` is copied from the app-log result
(or, for non-match, from the absence of a result while the system prompt kept
waiting).

| row | action | expected/proven result | runs |
|---|---|---|---|
| match | `emu finger touch 1` | `yes` | 3/3 |
| non-match | `emu finger touch 2` | no answer; the system prompt keeps waiting | 3/3 |
| cancel | leave the prompt unanswered for the kit's 30-second cancellation | `cancelled` | 3/3 |
| no fingerprint enrolled | launch on the unenrolled ATD image | `notAvailable` | 3/3 |
| lockout | five times `emu finger touch 2` from a fresh failure count | `lockedOut` | 3/3 |

Every `<row>-run<N>/` directory has:

- `logcat.txt` — the app's `NSHDEVAUTH` lines and the relevant framework or
  fingerprint-HAL lines;
- `prompt.png` — an emulator-console screenshot (for no-enrollment there is
  no system prompt to capture);
- `recording.webm` — an emulator-console recording;
- `verdict.txt` — the row result;
- `start.txt`, `finger.txt` where applicable, and capture command logs.

## Caveats

- API 35 only. API 29 and API 26 images were not proven in this run.
- The proof is emulator-only. OEM BiometricPrompt UI, real fingerprint
  hardware, strong/weak biometric classification on physical devices, and
  Samsung/Pixel behaviour are unproven.
- The no-enrollment rows prove the app-log result. There is no biometric
  system prompt in that state; early ATD screenshots can be black because the
  minimal ATD image was still finishing system startup while the app result
  had already been logged.
- Logcat's tag is the platform logger tag `nsh`; `NSHDEVAUTH` is the
  searchable message prefix, not the literal logcat tag.
