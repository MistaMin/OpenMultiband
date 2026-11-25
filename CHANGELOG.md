# Changelog

## 1.0.0 Beta (2026-11-25)

First public release.

- 1-to-6-band minimum-phase all-pass-matched Linkwitz-Riley crossover (12, 24 or 48 dB/oct), default 4 bands (120 Hz, 750 Hz, 3.00 kHz).
- Four pure-gain dynamics sections inside every band in signal order: Decompressor, Gate / Expander, Compressor and De-Expander, with automatic per-band latency alignment before summing.
- Per-band Gain (-24 to 24 dB), click-free Bypass, Solo and Mute, plus **Band Link** to adjust matching controls across all active bands together from any band.
- Global post-sum broadband Master Limiter (with Clean mode and Bass Hold), Input / Output Gain, Zero Latency, BS.1770 Auto Gain and phase-matched Delta listening mode.
- Full-band sidechain filtering (internal or external key through simultaneous high-pass, low-pass and bell filters) and look-ahead / auto timing on every per-band section.
- Hardware-accelerated software-rastered editor (1080 x 768 logical, 2x Retina): interactive crossover spectrum with draggable split handles, static transfer curve inside a 4-second level history, 3D spectrum view, collapsible slider panels and seven themes.
- 592 host parameters with whole-dB and whole-ratio readouts matching between editor and host.
- macOS 11 or later, universal (`arm64` + `x86_64`): VST3, Audio Unit, CLAP, LV2 and PACE Eden-signed AAX. Windows x64 and Linux x86_64: VST3, CLAP and LV2.
