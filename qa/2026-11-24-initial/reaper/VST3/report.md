# Plugin QA: VST3: OpenMultiband (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | VST3: OpenMultiband (OpenGrid), 596 parameters |
| finite output | PASS | peak -6.8 dBFS |
| not silent | PASS | rms -19.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x821, 2.4% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Input Gain |  | ok (-2.7 dB) | ok (5.930%) |
| 1 | Output Gain |  | ok (-0.6 dB) | ok (4.989%) |
| 2 | Zero Latency |  | ok (-4.1 dB) | ok (4.421%) |
| 3 | Auto Gain |  | ok (-5.3 dB) | ok (3.991%) |
| 4 | Delta |  | ok (-3.7 dB) | ok (3.717%) |
| 5 | Bands |  | ok (-7.1 dB) | ok (8.383%) |
| 6 | Crossover Slope |  | ok (-4.1 dB) | ok (4.640%) |
| 7 | Crossover 1 |  | ok (-4.5 dB) | ok (4.027%) |
| 8 | Crossover 2 |  | ok (-3.0 dB) | ok (3.729%) |
| 9 | Crossover 3 |  | ok (-3.7 dB) | ok (3.848%) |
| 10 | Crossover 4 | Bands=0.8 | ok (-6.3 dB) | excused (5.667%) |
| 11 | Crossover 5 | Bands=1 | ok (-5.7 dB) | excused (5.598%) |
| 12 | Limiter On |  | ok (-8.5 dB) | excused (4.546%) |
| 13 | Limiter Threshold |  | ok (-2.2 dB) | excused (3.899%) |
| 14 | Limiter Ceiling |  | ok (-7.4 dB) | excused (3.628%) |
| 15 | Limiter Attack | Limiter Threshold=0.5 | ok (-7.9 dB) | excused (6.975%) |
| 16 | Limiter Release | Limiter Clean=0, Limiter Threshold=0.5 | ok (-7.4 dB) | excused (4.402%) |
| 18 | Limiter Clean | Limiter Threshold=0.5 | ok (-20.9 dB) | excused (4.782%) |
| 19 | Limiter Bass Hold | Limiter Clean=1, Limiter Threshold=0.5 | ok (-29.2 dB) | excused (4.989%) |
| 22 | Band 1 Gain |  | ok (-1.4 dB) | ok (4.609%) |
| 23 | Band 1 Solo |  | ok (-9.1 dB) | ok (4.349%) |
| 24 | Band 1 Mute |  | ok (-15.1 dB) | ok (6.770%) |
| 25 | Band 1 Bypass |  | ok (-20.5 dB) | ok (3.064%) |
| 26 | B1 Gate On |  | ok (-49.5 dB) | excused (3.221%) |
| 27 | B1 Gate Mode | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-16.8 dB) | excused (3.074%) |
| 28 | B1 Gate Threshold | B1 Gate On=1 | ok (-15.1 dB) | excused (3.716%) |
| 30 | B1 Gate Range | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-15.2 dB) | excused (3.300%) |
| 48 | B1 Comp On |  | ok (-20.5 dB) | ok (3.180%) |
| 49 | B1 Comp Threshold |  | ok (-15.7 dB) | ok (3.918%) |
| 50 | B1 Comp Ratio |  | ok (-20.5 dB) | ok (3.735%) |
| 51 | B1 Comp Knee |  | ok (-36.3 dB) | excused (3.738%) |
| 52 | B1 Comp Attack |  | ok (-22.4 dB) | ok (3.674%) |
| 53 | B1 Comp Release |  | ok (-27.7 dB) | ok (3.446%) |
| 56 | B1 Comp Look-Ahead |  | ok (-6.0 dB) | excused (3.392%) |
| 61 | B1 Comp Make-Up |  | ok (-1.4 dB) | ok (4.364%) |
| 62 | B1 Comp Mix |  | ok (-17.8 dB) | ok (3.981%) |
| 68 | B1 DeExp On |  | ok (-52.1 dB) | excused (3.187%) |
| 69 | B1 DeExp Threshold | B1 DeExp On=1 | ok (-9.5 dB) | excused (4.238%) |
| 70 | B1 DeExp Ratio | B1 DeExp On=1 | ok (-46.9 dB) | excused (4.633%) |
| 71 | B1 DeExp Range | B1 DeExp On=1 | ok (-52.6 dB) | excused (4.422%) |
| 90 | B1 DeComp On |  | ok (-20.0 dB) | excused (4.313%) |
| 91 | B1 DeComp Threshold | B1 DeComp On=1 | ok (-20.0 dB) | excused (4.378%) |
| 92 | B1 DeComp Ratio | B1 DeComp On=1 | ok (-20.3 dB) | excused (4.659%) |
| 93 | B1 DeComp Range | B1 DeComp On=1 | ok (-19.5 dB) | excused (5.144%) |
| 117 | Band 2 Gain |  | ok (0.0 dB) | excused (5.774%) |
| 144 | B2 Comp Threshold |  | ok (-13.8 dB) | excused (3.781%) |
| 212 | Band 3 Gain |  | ok (-0.3 dB) | excused (3.954%) |
| 239 | B3 Comp Threshold |  | ok (-15.9 dB) | excused (3.658%) |
| 307 | Band 4 Gain |  | ok (-1.5 dB) | excused (3.713%) |
| 334 | B4 Comp Threshold |  | ok (-12.7 dB) | excused (3.534%) |
| 402 | Band 5 Gain | Bands=0.8 | ok (-0.6 dB) | excused (5.482%) |
| 429 | B5 Comp Threshold | Bands=0.8 | ok (-5.7 dB) | excused (5.098%) |
| 497 | Band 6 Gain | Bands=1 | ok (-0.1 dB) | excused (5.813%) |
| 524 | B6 Comp Threshold | Bands=1 | ok (-6.4 dB) | excused (5.475%) |
| 595 | Delta |  | ok (-4.4 dB) | ok (4.638%) |
