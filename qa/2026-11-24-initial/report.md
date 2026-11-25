# OpenMultiband Wiki QA Report (2026-11-24)

```text
Scope: 19 items, 19 verified (Gates 1-7 incl. REAPER VST3/CLAP/AU, OpenGrid9 VST3/AU/LV2, Pro Tools 2025.10 AAX + PTSL, and Band Link), 0 failed, 5 out of scope (Logic Pro & Studio One excluded by owner)
Builds: Release, macOS universal (arm64 + x86_64, modern minos 11.0) on OpenPluginSDK 2f24aee (au-snap-discrete-values) + GoodLookinDri local patch; VST3/AU/CLAP/LV2 installed to ~/Library/Audio/Plug-Ins/ (no /Library shadows); PACE-signed AAX (wrap GUID BD5F8E40-E806-11F1-8EC5-005056920FF7, Developer ID ZQYU67JUBH) installed to /Library/Application Support/Avid/Audio/Plug-Ins/ (SHA-256 a09ac4836ba35dfd...)
```

## Wiki QA Pipeline Checks (`Engineering Wiki/pages/processes.md`)

| Gate | Check | Result | Evidence |
|---|---|---|---|
| 1 Scope ledger | Intake + Autogain & Delta + PACE GUID + Band Link recorded | PASS | `qa/2026-11-24-initial/ledger.md` |
| 2 DSP audit | Crossover flat sum (< 0.005 dB across 1..6 bands, 12/24/48 dB/oct, 48/96 kHz), exact Delta unity null (< 1e-5), band latency alignment, click-free switches; DSP cost 2.0% core (4-band default) / 11.4% (6-band all 25 sections ON) | PASS | `cargo test --release --workspace`, `dsp_profile` |
| 3 GUI coverage, text audit & Band Link | 592/592 parameters bound; 0 overlaps/out-of-bounds; text >= 8 pt, 0 clipped/overlapping, 0 `-0`; `display_table` <= 3 digits & whole dB/ratio; `column_fill_matches_the_polygon` (< 0.5 mean diff, 0 seams); `knob_double_click_typing_and_drag_work` & `band_link_propagates_relative_and_discrete_across_active_bands` PASS; 7 offline + live REAPER/Pro Tools screenshots inspected | PASS | `cargo test --release --lib`, `display-table.txt`, `screenshots/*.png`, `protools/evidence/*.png` |
| 3 GUI perf gate (2x Retina) | Final build (with Band Link + GoodLookinDri patch): **7.93–14.09 ms/frame @ 2x** (B1 Comp `11.98 ms`, Sidechain `13.94 ms`, 3D `14.09 ms`, Limiter `7.93 ms` — all < 33 ms) | PASS | `frame_cost` (2026-11-24) |
| 3 On-screen host GPU/CPU | Visible host Activity Monitor / WindowServer measurement (3b) | SKIPPED | 2x pixel cost verified in-process by `frame_cost` (7.9–14.1 ms); live host CPU/GPU telemetry not collected |
| 4 Local suites | `cargo test --release --workspace` (99 tests across 4 crates + root lib, 0 warnings) | PASS | `cargo test` logs |
| 5 `auval` | `auval -v aufx OMbd Ogrd` (final Band Link build) | PASS | `AU VALIDATION SUCCEEDED.` |
| 5 `clap-validator` 0.4.1 | `~/Library/Audio/Plug-Ins/CLAP/OpenMultiband.clap` (final Band Link build) | PASS | 34 passed, 0 failed, 0 warnings, 10 skipped |
| 5 `pluginval` strictness 10 (VST3 & AU) | `~/Library/Audio/Plug-Ins/{VST3,Components}/OpenMultiband.*` (incl. Editor Automation & Fuzz) | PASS | `validators/pluginval-{vst3,au}.log` (`SUCCESS`) |
| 5 `AAX Validator` 2024.6 | `dist/signed/OpenMultiband.aaxplugin` (final signed Band Link build) | PASS | `validators/aax-final.log` (6/6 PASS; 0 duplicate param name warnings) |
| 6 REAPER VST3 | `VST3: OpenMultiband (OpenGrid)` (596 params, 55-param representative audio+visual sweep, state round-trip, 5 reopen cycles, 0 silent renders) | PASS | `reaper/VST3/report.md`, `reaper/VST3/editor_base_0.png` (`1080x821`, `-240.0 dB` det) |
| 6 REAPER CLAP | `CLAP: OpenMultiband (OpenGrid)` (595 params, 55-param sweep, state round-trip, 5 reopen cycles, 0 silent renders) | PASS | `reaper/CLAP/report.md`, `reaper/CLAP/editor_base_0.png` (`1080x821`, `-240.0 dB` det) |
| 6 REAPER AU | `AU: OpenMultiband (OpenGrid)` (`reaper/AU` caught discrete-snap mismatch on old SDK; rebuilt on SDK `2f24aee` -> `reaper/AU-2` 9/9 ALL PASS) | PASS | `reaper/AU-2/report.md`, `reaper/AU/editor_base_0.png` (`1080x821`, `-240.0 dB` det) |
| 6 OpenGrid9 VST3 / AU / LV2 | Headless matrix: VST3 PASS (`-240.0 dB`, 17/17 act, state PASS); AU audio + 592/592 snapped state PASS; LV2 loads & state PASS (known Ardour LV2 non-reset release tail `-17.2 dB` in first 500 ms) | PASS (with LV2 host note) | `opengrid9-suite/{VST3,AU,LV2}/report.md` |
| 7 Pro Tools 2025.10 AAX + PTSL | Scan, PACE auth, plugin cache, stereo+mono stems, live typing (`-36 dB`), drag (`Ratio 4:1->15:1`), **Band Link** (`B3 -36->-30` moved `B1 -18->-12`), BYPASS (`-20.0 dBFS`), PTSL `burnthrough.py exercise` PASS (`macOS-Native-48000-256`), offline-vs-realtime null **`-300.0 dB`**, wet-vs-bypass delta **`-14.91 dBFS`**, PTSL save/close/reopen recall (`B1 -12 dB`, `B3 -30 dB`, `GR -6 dB`), 0 stuck/DAE errors | PASS (`macOS-Native-48000-256`; 29 other rate/buffer matrix cells PENDING) | `protools/observations.md`, `protools/run-002/`, `protools/evidence/*.png` |
| 7 Logic Pro & Studio One | Excluded per owner ("I do not have logic and studio one is a pain") | N/A (out of scope) | Owner request (2026-11-24) |

## Bugs caught & fixes made during QA

1. `src/ui/kit.rs` — Centered symmetric progress bar in `Kind::Value` so `0 dB` Band Gain shows clean unity; removed per-frame Gaussian blur `shadow_ellipse` on `FaderH` thumbs; added `column_fill_matches_the_polygon` correctness test.
2. `src/ui/mod.rs` + `src/ui/view3d.rs` — Moved static non-panel control values, `MB_PLOT` band tint regions/trim lines/crossover badges, and 2D threshold dashes into the cached layer; skipped occluded history fills behind open settings panels; tuned 3D terrain offscreen slice clear & 18-row geometry. Reduced 2x frame cost from `19.7–35.8 ms` to `7.93–14.09 ms/frame`.
3. `src/lib.rs` — Shortened band parameter prefixes (`B{n} Gate / Comp / DeExp / DeComp`) after AAX Validator caught 31-char truncation collisions on Key Boost parameters; added 31-char uniqueness assertion to `params_are_unique_and_layout_is_complete`.
4. `Cargo.toml` + `OpenPluginSDK` `2f24aee` — REAPER AU initial run caught discrete-parameter state mismatch (`off -> on`); added `[patch."https://github.com/MistaMin/GoodLookinDri"]` and rebuilt on SDK `2f24aee` (`au-snap-discrete-values`); REAPER `AU-2` passed all 9/9 checks.
5. `src/ui/mod.rs` — Live Pro Tools testing caught that knob dragging and double-click text editing were swallowed because `build()` did not baseline `set.take_changed()` (causing `sync()` to see `P_BAND_COUNT` as "changed" and rebuild `ControlSet` on every `mouse_down`) and `mouse_up()` rebuilt unconditionally. Fixed both, added `knob_double_click_typing_and_drag_work`, and re-verified live in Pro Tools.
6. `src/ui/mod.rs` — Added **Band Link** (`LINK` toggle in band row, relative delta across all active bands for all per-band module controls & Band Gain, discrete switches/choices matched, Solo/Mute/Bypass independent, balanced host gestures, no cross-propagation on host automation), added `band_link_propagates_relative_and_discrete_across_active_bands`, and verified live in Pro Tools across save/close/reopen.

## Not done / known limits

- Remaining 29 Pro Tools rate/buffer matrix combinations, HDX, AudioSuite, automation lanes, and listening pass.
- REAPER suite was run on the build immediately prior to adding Band Link (all DSP & 592 host parameters unchanged; Band Link + knob fix verified by 99 unit tests, `auval`, `clap-validator`, `pluginval` strictness 10 VST3/AU, `AAX Validator`, and live Pro Tools 2025.10).
- Windows and Linux binaries are built via GitHub Actions and verified by the automated test suite only, not yet in a Windows or Linux DAW.
