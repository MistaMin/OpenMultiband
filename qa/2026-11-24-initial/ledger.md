# Scope ledger: OpenMultiband initial build, Band Link & Wiki QA

Project: OpenMultiband   Version: 1.0.0-beta   Started: 2026-11-24

Sources of scope:
- Request 1: "okay, let's take our OpenDynamics plugin and make a mutliband version of it and call it OpenMultibad" + "run the pluginqa from the wiki and update all skills to match the wiki"
- Request 2: "run the 6-7 Reaper and Target Hosts. however, I do not have logic and studio one is a pain. Pro Tools scripting works and is on the wiki on how to do it."
- Request 3: "I just realized that we don't have a band link function. Where we can control all bands from one threshhold,ratio inside a module"
- Owner answers (2026-11-24):
  - Name: OpenMultiband
  - Architecture: Full 4-section chain per band (Decompressor -> Gate/Expander -> Compressor -> De-Expander) + global broadband Limiter after band sum
  - Bands & crossover: 1 to 6 bands (starts at 4), Minimum-Phase Linkwitz-Riley crossover (12 / 24 / 48 dB/oct)
  - Globals: Keep both Auto Gain (K-weighted BS.1770) and Delta mode (latency-aligned wet minus dry) from OpenDynamics, plus Zero Latency, Input/Output Gain, 3D View, 7 Themes
  - Band Link: Global `LINK` toggle in band bar; relative delta across all active bands for every per-band module control (Decompressor, Gate/Expander, Compressor, De-Expander) plus Band Gain; switches/choices match; Solo/Mute/Bypass stay per-band
  - PACE Wrap GUID: `BD5F8E40-E806-11F1-8EC5-005056920FF7` (Signing Only)
  - Target hosts in scope: REAPER (`VST3`, `CLAP`, `AU`), OpenGrid9 (`VST3`, `AU`, `LV2`), Pro Tools 2025.10 (`AAX` + PTSL). Logic Pro (not installed) and Studio One excluded by owner.

## Items

| ID | Item | Source | Code | GUI | Test | Evidence | Status |
|---|---|---|---|---|---|---|---|
| S1 | Plugin identity (`OpenMultiband`, `com.opengrid.openmultiband`, AU `aufx OMbd Ogrd`, AAX `OpGd/OMbd/OMbM/OMbS`, LV2 URI, PACE GUID `BD5F8E40-E806-11F1-8EC5-005056920FF7`) | Request 1 + PACE | `Cargo.toml`, `src/lib.rs`, `scripts/build.sh` | Window header title | `auval`, `clap-validator`, `pluginval`, `aax_validate`, `wraptool verify`, Pro Tools `plugincache.txt` | `validators/*`, `protools/evidence/plugincache.txt` (2026-11-24) | verified |
| S2 | Linkwitz-Riley 1..=6 band crossover (12/24/48 dB/oct) with exact all-pass phase matching and flat magnitude sum (< 0.005 dB) | A3 | `src/crossover.rs` | Bands (`1..6`), Slope (`12/24/48 dB`), draggable crossover handles on `MB_PLOT` | `exact_sum_and_flat_magnitude_for_all_band_counts_and_slopes`, `bands_isolate_their_frequency_ranges`, `single_band_is_bit_exact_wire`, `rapid_frequency_automation_stays_finite_and_matched`, REAPER VST3/CLAP/AU sweeps | `cargo test --release --workspace`; `reaper/{VST3,CLAP,AU-2}/report.md` (2026-11-24) | verified |
| S3 | Per-band 4-section dynamics chain (`Decompressor -> Gate/Expander -> Compressor -> De-Expander`) across 6 bands with click-free 10 ms On/Off crossfade | A2 | `src/lib.rs` | Band selector bar (`B1..Bn`), 4 section tabs per band, per-section ON/main knobs/settings panels | `band_chain_order_is_decomp_first`, `params_are_unique_and_layout_is_complete` (incl. 31-char AAX check), crate suites (73 tests) | `cargo test --release --workspace`; `screenshots/*.png` (2026-11-24) | verified |
| S4 | Per-band latency alignment before summing bands (matching longest active band latency) | A2 + A3 | `src/lib.rs` | none: internal DSP alignment | `band_latency_alignment_sums_flat` | `cargo test --release --lib` (2026-11-24) | verified |
| S5 | Per-band Gain (-24..+24 dB whole steps), Solo, Mute, Bypass (click-free smoothed) | A2 + A3 | `src/lib.rs` | Selected band strip (`SOLO`, `MUTE`, `BYP`, `GAIN`) + band region highlight/trim line on `MB_PLOT` | `solo_mute_bypass_and_band_gain_act_independently`, REAPER VST3/CLAP/AU audio+visual sweep | `cargo test --release --lib`; `reaper/{VST3,CLAP,AU-2}/report.md` (2026-11-24) | verified |
| S6 | Global broadband Limiter post-band-sum with On toggle, Clean mode, Bass Hold, Auto Attack/Release | A2 | `src/lib.rs` | `MASTER LIMITER` tab, Limiter knobs/Clean/panels | `master_limiter_clamps_post_sum_peaks`, REAPER VST3/CLAP/AU sweep, `4band-limiter-dark-1x.png` | `cargo test --release --lib`; `reaper/{VST3,CLAP,AU-2}/report.md` (2026-11-24) | verified |
| S7 | Global Input Gain & Output Gain (-24..+24 dB whole steps) and IN/OUT sample-peak meters (24 dB/s fall) | OpenDynamics + plugin-qa | `src/lib.rs`, `src/ui/mod.rs` | Left/right rails: IN/OUT meters + Input/Output Small knobs | `meters_read_and_fall_as_specified`, REAPER VST3/CLAP/AU sweep, Pro Tools live meters | `cargo test --release --lib`; `reaper/VST3/editor_base_0.png`; `protools/evidence/03-stereo-1khz-default.png` | verified |
| S8 | Global Zero Latency mode (drops all look-aheads, reports 0 latency, 5 ms dip switch, dims lookahead controls) | OpenDynamics | `src/lib.rs`, `src/ui/mod.rs` | Header `ZERO LATENCY` pill | `zero_latency_reports_and_is_zero`, `zero_latency_and_band_count_switches_do_not_click`, REAPER VST3/CLAP/AU sweep | `cargo test --release --lib`; `reaper/{VST3,CLAP,AU-2}/report.md` (2026-11-24) | verified |
| S9 | Global Auto Gain (BS.1770 K-weighted 3 s loudness matching, +-24 dB clamp, -60 LUFS quiet hold) | A4 + plugin-scope-ledger | `src/autogain.rs`, `src/lib.rs` | Header `AUTO GAIN` pill + chip-row readout (whole dB, no `-0`) | `matches_the_input_loudness`, `off_is_a_wire`, REAPER VST3/CLAP/AU sweep | `cargo test --release --lib`; `reaper/{VST3,CLAP,AU-2}/report.md` (2026-11-24) | verified |
| S10 | Global Delta mode (phase- and latency-aligned wet minus dry, 50 ms glide, exact unity null across 1..6 bands & 12/24/48 dB/oct) | A4 + plugin-scope-ledger | `src/lib.rs`, `src/crossover.rs` | Header `DELTA` pill | `delta_nulls_at_unity_for_all_band_counts_and_slopes`, REAPER VST3/CLAP/AU sweep | `cargo test --release --lib`; `reaper/{VST3,CLAP,AU-2}/report.md` (2026-11-24) | verified |
| S11 | External stereo/mono sidechain routing + `KEY IN` indicator | OpenDynamics | `src/lib.rs`, `src/shared.rs` | Chip-row `KEY IN` readout + 2-column Sidechain panels + Pro Tools `no key input` selector | `external_sidechain_triggers_key_indicator_and_band_compression`, Pro Tools stereo header check | `cargo test --release --lib`; `4band-sidechain-dark-1x.png`; `protools/evidence/02-stereo-editor-pinknoise.png` | verified |
| S12 | Interactive multiband spectrum plot (live spectrum, band regions, per-band GR/gain overlay, click band to select, drag crossover handles) | A2 + A3 | `src/ui/mod.rs` | Top multiband plot (`MB_PLOT`) | `every_parameter_has_a_control`, `controls_stay_inside_window_and_do_not_overlap`, live REAPER & Pro Tools renders | `screenshots/*.png`, `protools/evidence/05-band-link-b3.png` (2026-11-24) | verified |
| S13 | Selected band/section curve + 4 s level/GR history + 3D spectrum terrain view + 7 themes | OpenDynamics | `src/ui/mod.rs`, `src/ui/view3d.rs`, `src/ui/theme.rs` | `DISPLAY` (2D & 3D), `3D VIEW` button, `THEME` button | `screenshots` across Dark, Light, Graphite, Paper & 3D view | `screenshots/4band-3d-dark-1x.png` (2026-11-24) | verified |
| S14 | GUI coverage, text audit, display rules (<=3 digits, whole dB/ratio, no 0.x except Key Boost Q, >=8 pt, no -0, no overlap/clip) + knob drag & double-click text edit | plugin-qa + plugin-gui-coverage | `src/ui/mod.rs`, `src/ui/kit.rs` | all pages, bands, panels, 2D & 3D | `every_parameter_has_a_control`, `controls_stay_inside_window_and_do_not_overlap`, `text_is_whole_apart_and_readable`, `display_table`, `dynamics_controls_move_in_whole_steps`, `column_fill_matches_the_polygon`, `knob_double_click_typing_and_drag_work`, live Pro Tools typing & drag | `display-table.txt`, `cargo test --release --lib`, `protools/evidence/04-stereo-typed-minus36.png` (2026-11-24) | verified |
| S15 | GUI performance at 2x Retina (< 33 ms/frame) & DSP profile | plugin-gui-performance | `src/ui/mod.rs`, `src/ui/view3d.rs`, `src/ui/kit.rs` | all tabs, panels, 3D | `frame_cost` final build: **7.93–14.09 ms/frame @ 2x** (Comp 11.98, Sidechain 13.94, 3D 14.09, Limiter 7.93 ms); `dsp_profile` (2.0% default / 11.4% 6-band all-on) | `frame_cost` & `dsp_profile` runs (2026-11-24) | verified |
| S16 | Universal macOS Release build (VST3, AU, CLAP, LV2, AAX; minos 11.0 modern), PACE-signed AAX & format validators | plugin-qa | `scripts/build.sh`, `Cargo.toml`, `.github/workflows/build.yml` | all formats | `build.sh`, `wraptool sign/verify`, `codesign --verify --deep --strict`, `auval`, `clap-validator` (34 pass / 0 fail), `pluginval` VST3 & AU (strictness 10), `aax_validate` (6/6, 0 dup-name warnings) | `validators/*` (2026-11-24) | verified |
| S17 | Gate 6 Host Matrix: REAPER (`VST3`, `CLAP`, `AU`) & OpenGrid9 (`VST3`, `AU`, `LV2`) | Request 2 + Wiki Gate 6 | installed bundles in `~/Library/Audio/Plug-Ins/` on SDK `2f24aee` | REAPER live editor (`1080x821`), 5 close/open cycles | `reaper_qa.py` VST3/CLAP/AU-2 (9/9 PASS each: loads, finite, non-silent, `-240.0 dB` deterministic, 55-param representative audio+visual sweep, state round-trip, 5 reopen cycles); `og9_qa.py` VST3/AU/LV2 + `og9qa_worker.lua` AU 592/592 | `reaper/{VST3,CLAP,AU-2}/report.md`, `opengrid9-suite/{VST3,AU,LV2}/report.md` (2026-11-24) | verified |
| S18 | Gate 7 Target Host: Pro Tools Studio 2025.10 (`25.10.0.144`) AAX + PTSL `burnthrough.py` | Request 2 + Wiki Gate 7 | PACE-signed AAX installed in `/Library/Application Support/Avid/Audio/Plug-Ins/` (SHA-256 `a09ac483...`) | Stereo & mono editors (`1080x871`), sidechain key selector, BYPASS, COMPARE, Mix peaks | Scan/auth/cache, stereo+mono stems, typing `-36 dB`, drag Ratio `4:1->15:1`, BYPASS `-20.0 dBFS`, PTSL `burnthrough.py exercise` PASS (`macOS-Native-48000-256`), offline-vs-realtime null **`-300.0 dB`**, wet-vs-bypass delta **`-14.91 dBFS`**, PTSL save/close/reopen recall (`B1 -12 dB`, `B3 -30 dB`, `GR -6 dB`), 0 stuck/DAE errors | `protools/observations.md`, `protools/run-002/`, `protools/evidence/*.png` (2026-11-24) | verified |
| S19 | Band Link (`LINK` toggle linking all active bands relatively across all per-band dynamics module controls & Band Gain; switches/modes match; Solo/Mute/Bypass independent) | Request 3 + Owner A2 | `src/ui/mod.rs` (`LINK_BUTTON`, `linked_peers`, `linked_value`, `propagate_link`, `end_link_drag`) | `LINK` button in band row (`750, 242, 72x28`), accented active band borders when linked, tooltip & accessibility switch | `band_link_propagates_relative_and_discrete_across_active_bands`, `controls_stay_inside_window_and_do_not_overlap`, `text_is_whole_apart_and_readable`, live Pro Tools verification (`B3 -36->-30` moved `B1 -18->-12` and survived PTSL save/close/reopen) | `cargo test --release --workspace`; `protools/evidence/{05-band-link-b3,06-band-link-b1,12-recalled-b1,13-recalled-b3}.png` (2026-11-24) | verified |

## Parameters (host list vs ledger)

Host list dumped from: OpenGrid9 VST3 (`593` = `592` plugin + `1` VST3 bypass), REAPER VST3 (`596`), REAPER CLAP (`595`), REAPER AU (`595`), OpenGrid9 AU (`592` plugin). Count: 592 plugin parameters. All in a row above: yes (`every_parameter_has_a_control` 592/592 PASS; `LINK` is a UI editing mode like `3D VIEW` so parameter IDs and saved sessions stay unchanged).

## Exceptions (each needs a reason)

| Parameter / check | Exception | Reason | Decided by |
|---|---|---|---|
| `b*.keyBoostQ` | 0.x decimal display allowed | EQ Q factor (standing house exception, same as OpenDynamics) | owner (2026-11-24) |
| REAPER visual sweep `--exceptions` | Hidden tabs/panels/inactive bands (`Bands 2..6`, `Limiter`, `Gate/DeExp/DeComp`, panel controls) excused for default-page visual diff | Verified statically across every band/section/panel by `every_parameter_has_a_control` (592/592) and screenshots | `reaper/exceptions.txt` (2026-11-24) |
| AU whole-step state check in `og9_qa.py` | `og9_qa.py` reports snap diff (`-16.32 -> -16`, etc.) | Expected host-bridge artefact for whole-step parameters (`plugin-daw-test` sec 2b); `og9qa_worker.lua` confirmed 592/592 exact readback after snap, and REAPER AU (`AU-2` on SDK `2f24aee`) passes state round-trip cleanly | verified by second route (2026-11-24) |
| LV2 repeat determinism in OpenGrid9 | `-17.2 dB` diff in first ~500 ms (`0.000000` after sample 40000) | Ardour/OpenGrid9 LV2 host does not call `lv2_deactivate`/`activate` between consecutive offline bounces, leaving compressor/limiter release tail from bounce 1; VST3, CLAP, AU (`-240.0 dB`) and Pro Tools AAX (`-300.0 dB`) all pass bit-exact | verified from WAV diff (2026-11-24) |

## Open questions

- Confirm whether to keep 1–6 bands (default 4) or cap at 5 bands.
- Confirm when to initialize the git repository (`MistaMin` identity) and run notarization / GitHub Release packaging.

## Out of scope

| Item | Decided by | Where |
|---|---|---|
| Linear-phase FIR crossover | User chose Minimum-Phase LR (A3) | Conversation intake |
| Per-band Limiter | User chose global broadband Limiter after band sum (A2) | Conversation intake |
| Logic Pro & Studio One host tests | User does not have Logic and excluded Studio One | Request 2 (2026-11-24) |
| Apple notarization, `.pkg` installer, GitHub Release upload, git commits/pushes | Standing QA rule 9 (requires owner approval) | plugin-qa |
| Windows / Linux builds | Workflow authored in `.github/workflows/build.yml`; not built on macOS | native-platform policy |
