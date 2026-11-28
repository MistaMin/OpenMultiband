//! TS-1: the conformance kit on OpenMultiband. Debug builds run Options::quick() (the Stop hook);
//! release builds run every rate and block size (the commit tester).

use openmultiband::OpenMultiband;
use openplugin::testing::{conformance, Options};

mod common;
use common::openmultiband_options;

openplugin::testing::install_allocator!();

#[test]
fn conforms() {
    let options = if cfg!(debug_assertions) {
        Options::quick()
    } else {
        Options::default()
    };
    conformance(OpenMultiband::default, openmultiband_options(options)).assert_pass();
}
