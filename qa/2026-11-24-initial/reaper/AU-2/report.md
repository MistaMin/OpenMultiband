# Plugin QA: AU: OpenMultiband (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | AU: OpenMultiband (OpenGrid), 595 parameters |
| finite output | PASS | peak -6.8 dBFS |
| not silent | PASS | rms -19.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x821, 13.8% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Input Gain |  | ok (-2.7 dB) | ok (4.917%) |
| 1 | Output Gain |  | ok (-0.6 dB) | ok (3.556%) |
| 2 | Zero Latency |  | ok (-5.2 dB) | ok (3.247%) |
| 3 | Auto Gain |  | ok (-5.3 dB) | ok (3.456%) |
| 4 | Delta |  | ok (-3.7 dB) | ok (3.275%) |
| 5 | Bands |  | ok (-7.1 dB) | ok (7.291%) |
| 6 | Crossover Slope |  | ok (-4.1 dB) | ok (3.439%) |
| 7 | Crossover 1 |  | ok (-4.5 dB) | ok (3.430%) |
| 8 | Crossover 2 |  | ok (-3.0 dB) | ok (3.929%) |
| 9 | Crossover 3 |  | ok (-3.7 dB) | ok (3.280%) |
| 10 | Crossover 4 | Bands=0.8 | ok (-6.3 dB) | excused (4.440%) |
| 11 | Crossover 5 | Bands=1 | ok (-5.7 dB) | excused (4.670%) |
| 12 | Limiter On |  | ok (-8.5 dB) | excused (3.957%) |
| 13 | Limiter Threshold |  | ok (-2.2 dB) | excused (3.371%) |
| 14 | Limiter Ceiling |  | ok (-7.4 dB) | excused (3.080%) |
| 15 | Limiter Attack | Limiter Threshold=0.5 | ok (-7.9 dB) | excused (4.430%) |
| 16 | Limiter Release | Limiter Clean=0, Limiter Threshold=0.5 | ok (-7.4 dB) | excused (4.462%) |
| 18 | Limiter Clean | Limiter Threshold=0.5 | ok (-20.9 dB) | excused (4.454%) |
| 19 | Limiter Bass Hold | Limiter Clean=1, Limiter Threshold=0.5 | ok (-29.2 dB) | excused (4.203%) |
| 22 | Band 1 Gain |  | ok (-1.4 dB) | ok (3.617%) |
| 23 | Band 1 Solo |  | ok (-9.1 dB) | ok (2.993%) |
| 24 | Band 1 Mute |  | ok (-15.1 dB) | ok (6.267%) |
| 25 | Band 1 Bypass |  | ok (-20.5 dB) | ok (2.587%) |
| 26 | B1 Gate On |  | ok (-49.5 dB) | excused (3.018%) |
| 27 | B1 Gate Mode | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-16.8 dB) | excused (3.388%) |
| 28 | B1 Gate Threshold | B1 Gate On=1 | ok (-15.1 dB) | excused (3.381%) |
| 30 | B1 Gate Range | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-15.2 dB) | excused (3.334%) |
| 48 | B1 Comp On |  | ok (-20.5 dB) | ok (2.756%) |
| 49 | B1 Comp Threshold |  | ok (-15.7 dB) | ok (3.494%) |
| 50 | B1 Comp Ratio |  | ok (-20.5 dB) | ok (3.411%) |
| 51 | B1 Comp Knee |  | ok (-36.3 dB) | excused (3.228%) |
| 52 | B1 Comp Attack |  | ok (-22.4 dB) | ok (3.295%) |
| 53 | B1 Comp Release |  | ok (-27.7 dB) | ok (3.253%) |
| 56 | B1 Comp Look-Ahead |  | ok (-6.0 dB) | excused (3.032%) |
| 61 | B1 Comp Make-Up |  | ok (-1.4 dB) | ok (3.419%) |
| 62 | B1 Comp Mix |  | ok (-20.5 dB) | ok (3.182%) |
| 68 | B1 DeExp On |  | ok (-52.1 dB) | excused (3.097%) |
| 69 | B1 DeExp Threshold | B1 DeExp On=1 | ok (-9.5 dB) | excused (4.127%) |
| 70 | B1 DeExp Ratio | B1 DeExp On=1 | ok (-49.1 dB) | excused (4.100%) |
| 71 | B1 DeExp Range | B1 DeExp On=1 | ok (-52.6 dB) | excused (3.923%) |
| 90 | B1 DeComp On |  | ok (-20.0 dB) | excused (3.845%) |
| 91 | B1 DeComp Threshold | B1 DeComp On=1 | ok (-20.0 dB) | excused (4.513%) |
| 92 | B1 DeComp Ratio | B1 DeComp On=1 | ok (-20.0 dB) | excused (4.619%) |
| 93 | B1 DeComp Range | B1 DeComp On=1 | ok (-19.5 dB) | excused (4.963%) |
| 117 | Band 2 Gain |  | ok (0.0 dB) | excused (4.961%) |
| 144 | B2 Comp Threshold |  | ok (-13.8 dB) | excused (2.980%) |
| 212 | Band 3 Gain |  | ok (-0.3 dB) | excused (3.274%) |
| 239 | B3 Comp Threshold |  | ok (-15.9 dB) | excused (3.092%) |
| 307 | Band 4 Gain |  | ok (-1.5 dB) | excused (3.827%) |
| 334 | B4 Comp Threshold |  | ok (-12.7 dB) | excused (3.052%) |
| 402 | Band 5 Gain | Bands=0.8 | ok (-0.6 dB) | excused (4.770%) |
| 429 | B5 Comp Threshold | Bands=0.8 | ok (-16.7 dB) | excused (4.230%) |
| 497 | Band 6 Gain | Bands=1 | ok (-0.1 dB) | excused (4.684%) |
| 524 | B6 Comp Threshold | Bands=1 | ok (-16.6 dB) | excused (4.839%) |
| 594 | Delta |  | ok (-4.4 dB) | ok (4.005%) |
