# OpenMultiband parameter reference

Generated from the plugin (`cargo test --release --lib dump_manual_params -- --ignored`). OpenMultiband exposes **592 host parameters**: 5 Global, 7 Crossover, 10 Master Limiter, and 95 per band across 6 bands (4 band strip controls plus 4 dynamics sections per band). Bands 2 through 6 use the exact same parameter names, ranges, and defaults as Band 1 (`Band 2`..`Band 6` and `B2`..`B6`).

## Global

| Parameter | Range or choices | Default |
|---|---|---|
| Input Gain | -24.0 dB to 24.0 dB | 0.0 dB |
| Output Gain | -24.0 dB to 24.0 dB | 0.0 dB |
| Zero Latency | Off, On | Off |
| Auto Gain | Off, On | Off |
| Delta | Off, On | Off |

## Crossover

| Parameter | Range or choices | Default |
|---|---|---|
| Bands | 1, 2, 3, 4, 5, 6 | 4 |
| Crossover Slope | 12 dB, 24 dB, 48 dB | 24 dB |
| Crossover 1 | 20.0 Hz to 20.00 kHz | 120.0 Hz |
| Crossover 2 | 20.0 Hz to 20.00 kHz | 750.0 Hz |
| Crossover 3 | 20.0 Hz to 20.00 kHz | 3.00 kHz |
| Crossover 4 | 20.0 Hz to 20.00 kHz | 8.00 kHz |
| Crossover 5 | 20.0 Hz to 20.00 kHz | 14.00 kHz |

## Master Limiter

| Parameter | Range or choices | Default |
|---|---|---|
| Limiter On | Off, On | On |
| Limiter Threshold | -24.0 dB to 0.0 dB | 0.0 dB |
| Limiter Ceiling | -24.0 dB to 0.0 dB | -1.0 dB |
| Limiter Attack | 0.0 ms to 10.0 ms | 0.0 ms |
| Limiter Release | 1.0 ms to 2000.0 ms | 100.0 ms |
| Limiter Stereo Link | 0% to 100% | 100% |
| Limiter Clean | Off, On | On |
| Limiter Bass Hold | 0.0 ms to 20.0 ms | 14.0 ms |
| Limiter Auto Attack | Off, On | Off |
| Limiter Auto Release | Off, On | Off |

## Per-band controls (Band 1 shown; identical for Bands 2 to 6)

| Parameter | Range or choices | Default |
|---|---|---|
| Band 1 Gain | -24.0 dB to 24.0 dB | 0.0 dB |
| Band 1 Solo | Off, On | Off |
| Band 1 Mute | Off, On | Off |
| Band 1 Bypass | Off, On | Off |

## Per-band Gate & Expander (Band 1 shown; identical for Bands 2 to 6)

| Parameter | Range or choices | Default |
|---|---|---|
| B1 Gate On | Off, On | Off |
| B1 Gate Mode | Gate, Expander | Gate |
| B1 Gate Threshold | -90.0 dB to 0.0 dB | -50.0 dB |
| B1 Gate Ratio | 1.0:1 to 20.0:1 | 2.0:1 |
| B1 Gate Range | 0.0 dB to 90.0 dB | 40.0 dB |
| B1 Gate Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| B1 Gate Hysteresis | 0.0 dB to 12.0 dB | 3.0 dB |
| B1 Gate Attack | 0.0 ms to 100.0 ms | 0.5 ms |
| B1 Gate Hold | 0.0 ms to 500.0 ms | 20.0 ms |
| B1 Gate Release | 5.0 ms to 5000.0 ms | 150.0 ms |
| B1 Gate Detector | Peak, RMS | Peak |
| B1 Gate RMS Window | 1.0 ms to 300.0 ms | 10.0 ms |
| B1 Gate Look-Ahead | 0.0 ms to 10.0 ms | 2.0 ms |
| B1 Gate Stereo Link | 0% to 100% | 100% |
| B1 Gate Key Source | Internal, External | Internal |
| B1 Gate Key High-Pass | 10.0 Hz to 2.00 kHz | 10.0 Hz |
| B1 Gate Key Low-Pass | 500.0 Hz to 22.00 kHz | 22.00 kHz |
| B1 Gate Auto Attack | Off, On | Off |
| B1 Gate Auto Release | Off, On | Off |
| B1 Gate Key Boost Freq | 20.0 Hz to 20.00 kHz | 1.00 kHz |
| B1 Gate Key Boost | -24.0 dB to 24.0 dB | 0.0 dB |
| B1 Gate Key Boost Q | 0.30 to 10.00 | 1.00 |

## Per-band Compressor (Band 1 shown; identical for Bands 2 to 6)

| Parameter | Range or choices | Default |
|---|---|---|
| B1 Comp On | Off, On | On |
| B1 Comp Threshold | -60.0 dB to 0.0 dB | -18.0 dB |
| B1 Comp Ratio | 1.0:1 to 30.0:1 | 4.0:1 |
| B1 Comp Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| B1 Comp Attack | 0.0 ms to 300.0 ms | 10.0 ms |
| B1 Comp Release | 5.0 ms to 5000.0 ms | 120.0 ms |
| B1 Comp Detector | Peak, RMS | Peak |
| B1 Comp RMS Window | 1.0 ms to 300.0 ms | 10.0 ms |
| B1 Comp Look-Ahead | 0.0 ms to 10.0 ms | 2.0 ms |
| B1 Comp Stereo Link | 0% to 100% | 100% |
| B1 Comp Key Source | Internal, External | Internal |
| B1 Comp Key High-Pass | 10.0 Hz to 2.00 kHz | 10.0 Hz |
| B1 Comp Key Low-Pass | 500.0 Hz to 22.00 kHz | 22.00 kHz |
| B1 Comp Make-Up | -12.0 dB to 24.0 dB | 0.0 dB |
| B1 Comp Mix | 0% to 100% | 100% |
| B1 Comp Auto Attack | Off, On | Off |
| B1 Comp Auto Release | Off, On | Off |
| B1 Comp Key Boost Freq | 20.0 Hz to 20.00 kHz | 1.00 kHz |
| B1 Comp Key Boost | -24.0 dB to 24.0 dB | 0.0 dB |
| B1 Comp Key Boost Q | 0.30 to 10.00 | 1.00 |

## Per-band De-Expander (Band 1 shown; identical for Bands 2 to 6)

| Parameter | Range or choices | Default |
|---|---|---|
| B1 DeExp On | Off, On | Off |
| B1 DeExp Threshold | -80.0 dB to 0.0 dB | -40.0 dB |
| B1 DeExp Ratio | 1.0:1 to 8.0:1 | 2.0:1 |
| B1 DeExp Range | 0.0 dB to 36.0 dB | 12.0 dB |
| B1 DeExp Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| B1 DeExp Floor | -inf dB to -20.0 dB | -72.0 dB |
| B1 DeExp Attack | 0.0 ms to 100.0 ms | 0.5 ms |
| B1 DeExp Hold | 0.0 ms to 500.0 ms | 20.0 ms |
| B1 DeExp Release | 1.0 ms to 5000.0 ms | 150.0 ms |
| B1 DeExp Detector | Peak, RMS | RMS |
| B1 DeExp RMS Window | 1.0 ms to 500.0 ms | 10.0 ms |
| B1 DeExp Look-Ahead | 0.0 ms to 10.0 ms | 2.0 ms |
| B1 DeExp Stereo Link | 0% to 100% | 100% |
| B1 DeExp Key Source | Internal, External | Internal |
| B1 DeExp Key High-Pass | 10.0 Hz to 2.00 kHz | 10.0 Hz |
| B1 DeExp Key Low-Pass | 500.0 Hz to 22.00 kHz | 22.00 kHz |
| B1 DeExp Mix | 0% to 100% | 100% |
| B1 DeExp Auto Attack | Off, On | Off |
| B1 DeExp Auto Release | Off, On | Off |
| B1 DeExp Key Boost Freq | 20.0 Hz to 20.00 kHz | 1.00 kHz |
| B1 DeExp Key Boost | -24.0 dB to 24.0 dB | 0.0 dB |
| B1 DeExp Key Boost Q | 0.30 to 10.00 | 1.00 |

## Per-band Decompressor (Band 1 shown; identical for Bands 2 to 6)

| Parameter | Range or choices | Default |
|---|---|---|
| B1 DeComp On | Off, On | Off |
| B1 DeComp Threshold | -60.0 dB to 0.0 dB | -18.0 dB |
| B1 DeComp Ratio | 1.0:1 to 4.0:1 | 2.0:1 |
| B1 DeComp Range | 0.0 dB to 24.0 dB | 6.0 dB |
| B1 DeComp Knee | 0.0 dB to 24.0 dB | 6.0 dB |
| B1 DeComp Attack | 0.0 ms to 100.0 ms | 1.0 ms |
| B1 DeComp Release | 1.0 ms to 2000.0 ms | 80.0 ms |
| B1 DeComp Detector | Peak, RMS, Transient | Peak |
| B1 DeComp RMS Window | 1.0 ms to 500.0 ms | 30.0 ms |
| B1 DeComp Transient Crest | 0.0 dB to 24.0 dB | 6.0 dB |
| B1 DeComp Look-Ahead | 0.0 ms to 10.0 ms | 2.0 ms |
| B1 DeComp Stereo Link | 0% to 100% | 100% |
| B1 DeComp Key Source | Internal, External | Internal |
| B1 DeComp Key High-Pass | 10.0 Hz to 2.00 kHz | 10.0 Hz |
| B1 DeComp Key Low-Pass | 500.0 Hz to 22.00 kHz | 22.00 kHz |
| B1 DeComp Mix | 0% to 100% | 100% |
| B1 DeComp Ceiling | Off, On | Off |
| B1 DeComp Ceiling Level | -24.0 dB to 0.0 dB | -1.0 dB |
| B1 DeComp Ceiling Type | Sample peak, True peak | True peak |
| B1 DeComp Output Trim | -24.0 dB to 0.0 dB | 0.0 dB |
| B1 DeComp Match | Off, On | Off |
| B1 DeComp Match Target Crest | 6.0 dB to 24.0 dB | 12.0 dB |
| B1 DeComp Auto Attack | Off, On | Off |
| B1 DeComp Auto Release | Off, On | Off |
| B1 DeComp Key Boost Freq | 20.0 Hz to 20.00 kHz | 1.00 kHz |
| B1 DeComp Key Boost | -24.0 dB to 24.0 dB | 0.0 dB |
| B1 DeComp Key Boost Q | 0.30 to 10.00 | 1.00 |
