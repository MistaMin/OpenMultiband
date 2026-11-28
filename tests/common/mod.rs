//! Options shared by tests/conformance.rs and tests/soak.rs.

use openplugin::testing::Options;

/// What OpenMultiband really does, for every run of the conformance kit.
pub fn openmultiband_options(options: Options) -> Options {
    Options {
        // Zero Latency, band count and crossover slope dip for 5 ms before switching, and per-band
        // look-ahead parameters move the reported latency.
        latency_depends_on_params: true,
        // ST-1: design ceiling with Master Limiter off across 6 bands (Input +24, Band +24,
        // Decompressor +24, Make-Up +24, De-Expander +36, Output +24, 6-band sum +16 dB = +172 dB).
        max_output: 10f32.powf(180.0 / 20.0),
        ..options
    }
}
