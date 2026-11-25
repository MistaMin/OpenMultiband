# Plugin QA: CLAP: OpenMultiband (OpenGrid)

| Check | Status | Detail |
|---|---|---|
| loads | PASS | CLAP: OpenMultiband (OpenGrid), 595 parameters |
| finite output | PASS | peak -6.8 dBFS |
| not silent | PASS | rms -19.9 dBFS |
| deterministic render | PASS | repeat differs by -240.0 dB |
| editor opens | PASS | 1080x821, 2.3% animates by itself |
| every parameter changes the sound | PASS |  |
| every parameter is shown in the GUI | PASS |  |
| state survives save and reopen | PASS |  |
| editor reopens | PASS | 5 close/open cycles |

| # | Parameter | Context | Audio | Visual |
|---|---|---|---|---|
| 0 | Input Gain |  | ok (-2.7 dB) | ok (4.367%) |
| 1 | Output Gain |  | ok (-0.6 dB) | ok (5.326%) |
| 2 | Zero Latency |  | ok (-4.1 dB) | ok (3.347%) |
| 3 | Auto Gain |  | ok (-5.3 dB) | ok (3.477%) |
| 4 | Delta |  | ok (-3.7 dB) | ok (3.393%) |
| 5 | Bands |  | ok (-7.1 dB) | ok (3.852%) |
| 6 | Crossover Slope |  | ok (-4.1 dB) | ok (3.465%) |
| 7 | Crossover 1 |  | ok (-4.5 dB) | ok (3.739%) |
| 8 | Crossover 2 |  | ok (-3.0 dB) | ok (3.265%) |
| 9 | Crossover 3 |  | ok (-3.7 dB) | ok (3.263%) |
| 10 | Crossover 4 | Bands=0.8 | ok (-6.3 dB) | excused (4.880%) |
| 11 | Crossover 5 | Bands=1 | ok (-5.7 dB) | excused (4.823%) |
| 12 | Limiter On |  | ok (-8.5 dB) | excused (4.939%) |
| 13 | Limiter Threshold |  | ok (-2.2 dB) | excused (3.395%) |
| 14 | Limiter Ceiling |  | ok (-7.4 dB) | excused (3.162%) |
| 15 | Limiter Attack | Limiter Threshold=0.5 | ok (-7.9 dB) | excused (4.889%) |
| 16 | Limiter Release | Limiter Clean=0, Limiter Threshold=0.5 | ok (-7.4 dB) | excused (4.737%) |
| 18 | Limiter Clean | Limiter Threshold=0.5 | ok (-20.9 dB) | excused (5.482%) |
| 19 | Limiter Bass Hold | Limiter Clean=1, Limiter Threshold=0.5 | ok (-29.2 dB) | excused (4.817%) |
| 22 | Band 1 Gain |  | ok (-1.4 dB) | ok (4.835%) |
| 23 | Band 1 Solo |  | ok (-9.1 dB) | ok (3.280%) |
| 24 | Band 1 Mute |  | ok (-15.1 dB) | ok (7.198%) |
| 25 | Band 1 Bypass |  | ok (-20.5 dB) | ok (2.645%) |
| 26 | B1 Gate On |  | ok (-49.5 dB) | excused (3.437%) |
| 27 | B1 Gate Mode | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-16.8 dB) | excused (3.923%) |
| 28 | B1 Gate Threshold | B1 Gate On=1 | ok (-15.1 dB) | excused (3.914%) |
| 30 | B1 Gate Range | B1 Gate On=1, B1 Gate Threshold=0.9 | ok (-15.2 dB) | excused (3.546%) |
| 48 | B1 Comp On |  | ok (-20.5 dB) | ok (2.895%) |
| 49 | B1 Comp Threshold |  | ok (-15.7 dB) | ok (3.592%) |
| 50 | B1 Comp Ratio |  | ok (-20.5 dB) | ok (3.854%) |
| 51 | B1 Comp Knee |  | ok (-36.3 dB) | excused (3.327%) |
| 52 | B1 Comp Attack |  | ok (-22.4 dB) | ok (3.080%) |
| 53 | B1 Comp Release |  | ok (-27.7 dB) | ok (3.889%) |
| 56 | B1 Comp Look-Ahead |  | ok (-5.3 dB) | excused (3.371%) |
| 61 | B1 Comp Make-Up |  | ok (-1.4 dB) | ok (5.191%) |
| 62 | B1 Comp Mix |  | ok (-17.8 dB) | ok (3.620%) |
| 68 | B1 DeExp On |  | ok (-52.1 dB) | excused (4.222%) |
| 69 | B1 DeExp Threshold | B1 DeExp On=1 | ok (-9.5 dB) | excused (4.616%) |
| 70 | B1 DeExp Ratio | B1 DeExp On=1 | ok (-46.9 dB) | excused (5.567%) |
| 71 | B1 DeExp Range | B1 DeExp On=1 | ok (-52.6 dB) | excused (4.505%) |
| 90 | B1 DeComp On |  | ok (-20.0 dB) | excused (4.121%) |
| 91 | B1 DeComp Threshold | B1 DeComp On=1 | ok (-20.0 dB) | excused (4.892%) |
| 92 | B1 DeComp Ratio | B1 DeComp On=1 | ok (-20.3 dB) | excused (5.794%) |
| 93 | B1 DeComp Range | B1 DeComp On=1 | ok (-19.5 dB) | excused (5.232%) |
| 117 | Band 2 Gain |  | ok (0.0 dB) | excused (5.746%) |
| 144 | B2 Comp Threshold |  | ok (-13.8 dB) | excused (3.874%) |
| 212 | Band 3 Gain |  | ok (-0.3 dB) | excused (4.666%) |
| 239 | B3 Comp Threshold |  | ok (-15.9 dB) | excused (4.165%) |
| 307 | Band 4 Gain |  | ok (-1.5 dB) | excused (4.258%) |
| 334 | B4 Comp Threshold |  | ok (-12.7 dB) | excused (4.352%) |
| 402 | Band 5 Gain | Bands=0.8 | ok (-0.6 dB) | excused (5.831%) |
| 429 | B5 Comp Threshold | Bands=0.8 | ok (-5.7 dB) | excused (5.275%) |
| 497 | Band 6 Gain | Bands=1 | ok (-0.1 dB) | excused (5.636%) |
| 524 | B6 Comp Threshold | Bands=1 | ok (-6.4 dB) | excused (5.637%) |
| 594 | Delta |  | ok (-4.4 dB) | ok (4.548%) |
