# Plugin QA: VST3: OpenMultiband

| Check | Status | Detail |
|---|---|---|
| loads | PASS | OpenMultiband, 593 parameters |
| finite output | PASS | peak -9.0 dBFS |
| not silent | PASS | rms -19.6 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | SKIPPED | visual checks disabled |
| every parameter changes the sound | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | SKIPPED | headless: OpenGrid9 runs without an editor window |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Input Gain |  | ok (-1.7 dB) | -  |
| 1 | Output Gain |  | ok (23.0 dB) | -  |
| 2 | Zero Latency |  | ok (23.1 dB) | -  |
| 3 | Auto Gain |  | ok (23.0 dB) | -  |
| 4 | Delta |  | ok (40.8 dB) | -  |
| 5 | Bands |  | ok (23.0 dB) | -  |
| 6 | Crossover Slope |  | ok (23.1 dB) | -  |
| 7 | Crossover 1 |  | ok (23.1 dB) | -  |
| 12 | Limiter On |  | ok (34.1 dB) | -  |
| 13 | Limiter Threshold |  | ok (23.2 dB) | -  |
| 14 | Limiter Ceiling |  | ok (24.1 dB) | -  |
| 22 | Band 1 Gain |  | ok (24.1 dB) | -  |
| 23 | Band 1 Solo |  | ok (24.1 dB) | -  |
| 24 | Band 1 Mute |  | ok (24.1 dB) | -  |
| 25 | Band 1 Bypass |  | ok (24.1 dB) | -  |
| 49 | B1 Comp Threshold |  | ok (24.1 dB) | -  |
| 50 | B1 Comp Ratio |  | ok (24.1 dB) | -  |
