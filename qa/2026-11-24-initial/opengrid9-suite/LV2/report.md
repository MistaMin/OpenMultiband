# Plugin QA: LV2: OpenMultiband

| Check | Status | Detail |
|---|---|---|
| loads | PASS | OpenMultiband, 600 parameters |
| finite output | PASS | peak -9.1 dBFS |
| not silent | PASS | rms -19.6 dBFS |
| deterministic render | FAIL | repeat differs by -17.2 dB |
| editor opens | SKIPPED | visual checks disabled |
| every parameter changes the sound | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | SKIPPED | headless: OpenGrid9 runs without an editor window |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 4 | Input Gain |  | skipped (render not deterministic)  | -  |
| 5 | Output Gain |  | skipped (render not deterministic)  | -  |
| 6 | Zero Latency |  | skipped (render not deterministic)  | -  |
| 7 | Auto Gain |  | skipped (render not deterministic)  | -  |
| 8 | Delta |  | skipped (render not deterministic)  | -  |
| 9 | Bands |  | skipped (render not deterministic)  | -  |
| 10 | Crossover Slope |  | skipped (render not deterministic)  | -  |
| 11 | Crossover 1 |  | skipped (render not deterministic)  | -  |
| 16 | Limiter On |  | skipped (render not deterministic)  | -  |
| 17 | Limiter Threshold |  | skipped (render not deterministic)  | -  |
| 18 | Limiter Ceiling |  | skipped (render not deterministic)  | -  |
| 26 | Band 1 Gain |  | skipped (render not deterministic)  | -  |
| 27 | Band 1 Solo |  | skipped (render not deterministic)  | -  |
| 28 | Band 1 Mute |  | skipped (render not deterministic)  | -  |
| 29 | Band 1 Bypass |  | skipped (render not deterministic)  | -  |
| 53 | B1 Comp Threshold |  | skipped (render not deterministic)  | -  |
| 54 | B1 Comp Ratio |  | skipped (render not deterministic)  | -  |
