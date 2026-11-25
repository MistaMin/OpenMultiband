# Third-party licenses

OpenMultiband's own source code is MIT licensed (see `LICENSE`). The official
binaries are proprietary freeware (see `BINARY_LICENSE.txt`). That is not a
relicensing of the components below, which keep their own terms. The installer
carries copies of the licence texts in `Licenses/`.

| Component | Used for | License |
|---|---|---|
| OpenPluginSDK (`openplugin`, `openplugin-core`, `openplugin-dri`, `openplugin-dsp`, `openplugin-host`, `openplugin-targets`) | plugin framework; CLAP and LV2 targets and VST3, Audio Unit and AAX shells | MIT or Apache License 2.0 |
| GoodLookinDri (`goodlookindri`, `goodlookindri-shell`) | editor drawing and windowing (Rust, linked statically) | MIT |
| Source Sans Pro (Adobe) | editor lettering, embedded in GoodLookinDri | SIL Open Font License 1.1 (`Licenses/SourceSansPro-OFL.txt`) |
| Rust standard library | all code | MIT or Apache License 2.0 |
| fontdue, ttf-parser | lettering | MIT or Apache License 2.0 (fontdue also Zlib) |
| rustfft, num-complex, num-integer, num-traits, primal-check, strength_reduce, transpose | FFTs for the crossover spectrum, spectrograph and 3D view | MIT or Apache License 2.0 |
| ahash, allocator-api2, cfg-if, hashbrown, itoa, log, memchr, once_cell, serde, serde_core, serde_derive, serde_json, zerocopy, zmij | support and state serialization libraries | MIT or Apache License 2.0 (memchr also Unlicense; zerocopy also BSD 2-Clause; zmij MIT) |
| libc, bitflags, windows-sys, windows-link | system interfaces (per platform) | MIT or Apache License 2.0 |
| x11rb, rustix, errno, linux-raw-sys, gethostname | Linux system interfaces | MIT or Apache License 2.0 (linux-raw-sys also Apache-2.0 WITH LLVM-exception) |
| proc-macro2, quote, syn, unicode-ident | compile-time procedural macros (`serde_derive`) | MIT or Apache License 2.0 (unicode-ident also Unicode-3.0) |
| CLAP | CLAP format | MIT |
| LV2 | LV2 format (interface written from the specification; no LV2 code linked) | ISC |
| VST3 SDK (Steinberg) | VST3 format | MIT |
| AudioUnitSDK (Apple) | Audio Unit format | Apache License 2.0 |
| AAX SDK (Avid) | AAX format | Avid AAX Developer Agreement / SDK License (`Licenses/third-party/AAX-SDK/LICENSE.txt`) |

The AAX SDK is an external proprietary dependency under the Avid AAX Developer
Agreement and is not committed to this repository. Official AAX binaries are
digitally signed with PACE Eden for Pro Tools compatibility.

VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a
trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology,
Inc. Other format and host names are used only to describe compatibility.
