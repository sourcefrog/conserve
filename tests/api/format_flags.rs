// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

//! Tests for per-band format flags.

use assert_matches::assert_matches;

use conserve::test_fixtures::ScratchArchive;
use conserve::*;

#[test]
fn unknown_format_flag_fails_to_open() {
    let af = ScratchArchive::new();

    let orig_band = Band::create_with_flags(&af, &["wibble".into()]).unwrap();
    assert_eq!(orig_band.format_flags(), ["wibble"]);

    let err = Band::open(&af, orig_band.id()).unwrap_err();
    println!("{err}");
    assert_matches!(err, Error::UnsupportedBandFormatFlags { .. });
    assert!(err
        .to_string()
        .starts_with(r#"Band b0000 has feature flags ["wibble"] not supported by Conserve "#));
}
