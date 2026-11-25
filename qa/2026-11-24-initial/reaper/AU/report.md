# Plugin QA: AU: OpenMultiband (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | AU: OpenMultiband (OpenGrid), 595 parameters |
| finite output | PASS | peak -6.8 dBFS |
| not silent | PASS | rms -19.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x821, 13.9% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | FAIL | Zero Latency (off -> on), Delta (off -> on), Crossover Slope (12 dB -> 24 dB), Limiter On (off -> on), Band 1 Solo (off -> on), Band 1 Mute (off -> on), B1 Gate On (off -> on), B1 DeComp On (off -> on) |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Input Gain |  | ok (-2.7 dB) | ok (5.324%) |
| 1 | Output Gain |  | ok (-0.6 dB) | ok (4.184%) |
| 2 | Zero Latency |  | ok (-5.2 dB) | ok (3.421%) |
| 3 | Auto Gain |  | ok (-5.3 dB) | ok (3.507%) |
| 4 | Delta |  | ok (-3.7 dB) | ok (3.550%) |
| 5 | Bands |  | ok (-7.1 dB) | ok (7.421%) |
| 6 | Crossover Slope |  | ok (-4.1 dB) | ok (3.354%) |
| 7 | Crossover 1 |  | ok (-4.5 dB) | ok (3.443%) |
| 8 | Crossover 2 |  | ok (-3.0 dB) | ok (3.428%) |
| 9 | Crossover 3 |  | ok (-3.7 dB) | ok (3.744%) |
| 10 | Crossover 4 | Bands=0.8 | ok (-6.3 dB) | excused (5.054%) |
| 11 | Crossover 5 | Bands=1 | ok (-5.7 dB) | excused (4.902%) |
| 12 | Limiter On |  | ok (-8.5 dB) | excused (3.940%) |
| 13 | Limiter Threshold |  | ok (-2.2 dB) | excused (3.601%) |
| 14 | Limiter Ceiling |  | ok (-7.4 dB) | excused (3.838%) |
| 15 | Limiter Attack | Limiter Threshold=0.5 | ok (-7.9 dB) | excused (7.046%) |
| 16 | Limiter Release | Limiter Clean=0, Limiter Threshold=0.5 | ok (-7.4 dB) | excused (4.370%) |
| 18 | Limiter Clean | Limiter Threshold=0.5 | ok (-20.9 dB) | excused (4.573%) |
| 19 | Limiter Bass Hold | Limiter Clean=1, Limiter Threshold=0.5 | ok (-29.2 dB) | excused (5.147%) |
| 22 | Band 1 Gain |  | ok (-1.4 dB) | ok (4.978%) |
| 23 | Band 1 Solo |  | ok (-9.1 dB) | ok (3.616%) |
| 24 | Band 1 Mute |  | ok (-15.1 dB) | ok (6.624%) |
| 25 | Band 1 Bypass |  | ok (-20.5 dB) | ok (2.784%) |
| 26 | B1 Gate On |  | ok (-49.5 dB) | excused (3.639%) |
| 27 | B1 Gate Mode | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-16.8 dB) | excused (2.959%) |
| 28 | B1 Gate Threshold | B1 Gate On=1 | ok (-15.1 dB) | excused (3.265%) |
| 30 | B1 Gate Range | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-15.2 dB) | excused (3.815%) |
| 48 | B1 Comp On |  | ok (-20.5 dB) | ok (2.877%) |
| 49 | B1 Comp Threshold |  | ok (-15.7 dB) | ok (3.725%) |
| 50 | B1 Comp Ratio |  | ok (-20.5 dB) | ok (4.125%) |
| 51 | B1 Comp Knee |  | ok (-36.3 dB) | excused (3.083%) |
| 52 | B1 Comp Attack |  | ok (-22.4 dB) | ok (3.852%) |
| 53 | B1 Comp Release |  | ok (-27.7 dB) | ok (3.603%) |
| 56 | B1 Comp Look-Ahead |  | ok (-6.0 dB) | excused (3.316%) |
| 61 | B1 Comp Make-Up |  | ok (-1.4 dB) | ok (5.103%) |
| 62 | B1 Comp Mix |  | ok (-20.5 dB) | ok (3.717%) |
| 68 | B1 DeExp On |  | ok (-52.1 dB) | excused (4.228%) |
| 69 | B1 DeExp Threshold | B1 DeExp On=1 | ok (-9.5 dB) | excused (4.543%) |
| 70 | B1 DeExp Ratio | B1 DeExp On=1 | ok (-49.1 dB) | excused (4.472%) |
| 71 | B1 DeExp Range | B1 DeExp On=1 | ok (-52.6 dB) | excused (4.908%) |
| 90 | B1 DeComp On |  | ok (-20.0 dB) | excused (4.359%) |
| 91 | B1 DeComp Threshold | B1 DeComp On=1 | ok (-20.0 dB) | excused (4.374%) |
| 92 | B1 DeComp Ratio | B1 DeComp On=1 | ok (-20.0 dB) | excused (4.482%) |
| 93 | B1 DeComp Range | B1 DeComp On=1 | ok (-19.5 dB) | excused (4.072%) |
| 117 | Band 2 Gain |  | ok (0.0 dB) | excused (4.031%) |
| 144 | B2 Comp Threshold |  | ok (-13.8 dB) | excused (3.652%) |
| 212 | Band 3 Gain |  | ok (-0.3 dB) | excused (3.304%) |
| 239 | B3 Comp Threshold |  | ok (-15.9 dB) | excused (3.397%) |
| 307 | Band 4 Gain |  | ok (-1.5 dB) | excused (4.202%) |
| 334 | B4 Comp Threshold |  | ok (-12.7 dB) | excused (3.280%) |
| 402 | Band 5 Gain | Bands=0.8 | ok (-0.6 dB) | excused (4.646%) |
| 429 | B5 Comp Threshold | Bands=0.8 | ok (-16.7 dB) | excused (5.588%) |
| 497 | Band 6 Gain | Bands=1 | ok (-0.1 dB) | excused (5.170%) |
| 524 | B6 Comp Threshold | Bands=1 | ok (-16.6 dB) | excused (4.964%) |
| 594 | Delta |  | ok (-4.4 dB) | ok (3.837%) |
