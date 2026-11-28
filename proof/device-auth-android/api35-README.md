driver: adb emu screenrecord + framework biometric prompts; app examples/device-auth copies; kit nsh commit dd3366e12004e73faacc7612f297e6feec62a9fc; taffy 0.15.0

free before image/boot: 29G
free after image rows: 26G

| row | availability | result | expected | crash | verdict |
|---|---|---|---|---|---|
| noperm-launch | none | notAvailable | notAvailable | no | pass |
| no-enrollment | none | notAvailable | notAvailable | no | pass |
| match | biometric | yes | yes | no | pass |
| nonmatch | biometric | no answer |  | no | pass |
| cancel | biometric | cancelled | cancelled | no | pass |
| lockout | biometric | lockedOut | lockedOut | no | pass |
| timeout | biometric | cancelled | cancelled | no | pass |
