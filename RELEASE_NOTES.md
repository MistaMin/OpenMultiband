# OpenMultiband 1.0.0 Beta

First public pre-release of **OpenMultiband**, a 1-to-6-band Linkwitz-Riley multiband dynamics processor by OpenGrid.

## What's in this release

- **All-pass-matched Linkwitz-Riley crossover:** 1 to 6 bands (default 4) at 12, 24, or 48 dB/oct, with interactive draggable split handles and phase-matched dry reference for clean Delta nulling.
- **Four dynamics sections per band:** Decompressor, Gate / Expander, Compressor, and De-Expander in signal order, each a pure gain stage with exact static curves, look-ahead, auto attack/release, and internal/external sidechain filtering (HP + LP + bell).
- **Per-band routing & Band Link:** automatic band latency alignment before summing, per-band Gain, Solo, Mute, and click-free Bypass, plus **Band Link** (`LINK`) for relative multi-band editing from any band while keeping Solo/Mute/Bypass and host automation independent.
- **Master stage & listening aids:** global post-sum broadband Master Limiter with Clean mode and Bass Hold, Zero Latency switch, BS.1770 Auto Gain, and Delta mode.
- **Editor:** interactive crossover spectrum, static curve in 4-second level history, 3D spectrum view, collapsible panels, double-click numeric entry, and seven themes.

## Formats & requirements

- **macOS 11.0 or later (Universal `arm64` + `x86_64`):** VST3, Audio Unit (AU), CLAP, LV2, and PACE Eden-signed AAX (Pro Tools). Developer ID signed and Apple notarized `.pkg` installer.
- **Windows 10 or later (`x64`):** VST3, CLAP, and LV2 via `OpenMultiband-1.0.0-Beta-windows-x64-setup.exe` (built via GitHub Actions; unsigned, SmartScreen may prompt).
- **Linux (`x86_64`, glibc 2.35+):** VST3, CLAP, and LV2 via `OpenMultiband-1.0.0-Beta-linux-x86_64.tar.gz` (built via GitHub Actions).

## Installation

- **macOS:** Run `OpenMultiband-1.0.0-Beta-macos.pkg` and pick the formats you want; the User Manual PDF and licence texts install into `/Library/Application Support/OpenMultiband`.
- **Windows:** Run `OpenMultiband-1.0.0-Beta-windows-x64-setup.exe` and choose VST3, CLAP and/or LV2.
- **Linux:** Extract `OpenMultiband-1.0.0-Beta-linux-x86_64.tar.gz` (`usr/lib/vst3`, `usr/lib/clap`, `usr/lib/lv2`, `usr/share/doc/openmultiband`).

## Verification passed

- **Automated tests:** 99 Rust unit, DSP, layout, readability, text-entry, and Band Link tests pass (0 warnings) on macOS, Windows x64, and Linux x86_64; 2x Retina frame benchmark under 14.1 ms/frame on all views.
- **Format validators (macOS):** `auval` (`aufx OMbd Ogrd`), `clap-validator`, `pluginval` (strictness 10, VST3 & AU), and `AAX Validator` all pass.
- **Hosts tested (macOS):** REAPER (VST3, CLAP, Audio Unit — audio sweep, state round-trip, deterministic null, editor lifecycle), OpenGrid9 (VST3, AU, LV2), and Pro Tools Studio 2025.10 (AAX — live knob drag/typing, Band Link, bypass, PTSL burnthrough, bit-exact offline vs real-time null, session save/reopen recall).

## Known limits

- **Windows & Linux host testing:** Windows and Linux binaries are built by GitHub Actions and verified by the automated Rust test suite only, not yet in a Windows or Linux DAW.
- **Windows code signing:** The Windows installer is not code-signed, so SmartScreen may ask before running.
- **No standalone app:** plugin formats only.
- **Untested macOS hosts:** Logic Pro and Studio One were not tested.
- First public beta (`pre-release`); report issues on GitHub.

## Release files

- `OpenMultiband-1.0.0-Beta-macos.pkg` (+ `.sha256`) — signed, notarized universal macOS installer (VST3, AU, CLAP, LV2, AAX, manual & licences)
- `OpenMultiband-1.0.0-Beta-windows-x64-setup.exe` (+ `.sha256`) — Windows x64 installer (VST3, CLAP, LV2)
- `OpenMultiband-1.0.0-Beta-linux-x86_64.tar.gz` (+ `.sha256`) — Linux x86_64 archive (VST3, CLAP, LV2)
- `OpenMultiband-User-Manual.pdf` — full user manual and parameter reference
- `opengrid-manifest.json` — OpenGrid Plugin Manager manifest (schema 2)
