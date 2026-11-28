driver: adb emu screenrecord + framework biometric prompts; app examples/device-auth copies; kit nsh commit dd3366e12004e73faacc7612f297e6feec62a9fc; taffy 0.15.0

free before image/boot: 8G
free after image rows: 7G

| row | availability | result | expected | crash | verdict |
|---|---|---|---|---|---|
| bio-match | biometric | yes | yes | no | pass |
| bio-cancel | biometric | cancelled | cancelled | no | pass |
| cred-match | biometric | yes | yes | no | pass |
| cred-cancel | biometric | cancelled | cancelled | no | pass |
| cred-use-pin-negative | biometric | no | no | no | pass |
