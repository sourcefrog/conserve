extern crate vergen;

use vergen::*;

fn main() {
    let mut flags = OutputFns::empty();
    flags.toggle(SEMVER);
    // Generate the version.rs file in the Cargo OUT_DIR.
    assert!(vergen(flags).is_ok());
}
