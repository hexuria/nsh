driver: adb emu screenrecord + framework biometric prompts; kit nsh commit 3675941284d2c8cf36c12abfee65dd98e369ce64; taffy 0.15.0

| row | availability | result | expected | crash | verdict |
|---|---|---|---|---|---|
| noperm-launch | none | notAvailable | notAvailable | no | pass |
| bio-match | biometric | yes | yes | no | pass |
| bio-cancel | biometric | cancelled | cancelled | no | pass |
| cred-match | biometric | yes | yes | no | pass |
| cred-cancel | biometric | cancelled | cancelled | no | pass |
| cred-use-pin-negative | biometric | no | no | no | pass |
