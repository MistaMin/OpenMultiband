# Plugin QA: AU: OpenMultiband

| Check | Status | Detail |
|---|---|---|
| loads | PASS | OpenMultiband, 592 parameters |
| finite output | PASS | peak -9.0 dBFS |
| not silent | PASS | rms -19.6 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | SKIPPED | visual checks disabled |
| every parameter changes the sound | PASS |  |
| state survives save and reopen | FAIL | Input Gain (-16.32 -> -16), Output Gain (-0.960001 -> -1), Bands (4.91667 -> 5), Limiter Threshold (-12.48 -> -12), Limiter Ceiling (-4.8 -> -5), Band 1 Gain (-16.32 -> -16), B1 Comp Ratio (1.36233 -> 1) |
| editor reopens | SKIPPED | headless: OpenGrid9 runs without an editor window |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Input Gain |  | ok (-3.0 dB) | -  |
| 1 | Output Gain |  | ok (11.5 dB) | -  |
| 2 | Zero Latency |  | ok (-7.1 dB) | -  |
| 3 | Auto Gain |  | ok (-13.7 dB) | -  |
| 4 | Delta |  | ok (-7.0 dB) | -  |
| 5 | Bands |  | ok (-9.0 dB) | -  |
| 6 | Crossover Slope |  | ok (-5.0 dB) | -  |
| 7 | Crossover 1 |  | ok (-5.7 dB) | -  |
| 12 | Limiter On |  | ok (-6.5 dB) | -  |
| 13 | Limiter Threshold |  | ok (-0.7 dB) | -  |
| 14 | Limiter Ceiling |  | ok (-6.5 dB) | -  |
| 22 | Band 1 Gain |  | ok (-4.6 dB) | -  |
| 23 | Band 1 Solo |  | ok (-6.9 dB) | -  |
| 24 | Band 1 Mute |  | ok (-6.9 dB) | -  |
| 25 | Band 1 Bypass |  | ok (-6.9 dB) | -  |
| 49 | B1 Comp Threshold |  | ok (-6.9 dB) | -  |
| 50 | B1 Comp Ratio |  | ok (-6.9 dB) | -  |
