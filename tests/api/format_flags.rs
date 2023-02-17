// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

//! Tests for per-band format flags.

use assert_matches::assert_matches;

use conserve::test_fixtures::ScratchArchive;
use conserve::*;

#[test]
// This can be updated if/when Conserve does start writing some flags by default.
fn default_format_flags_are_empty() {
    let af = ScratchArchive::new();

    let orig_band = Band::create(&af).unwrap();
    let flags = orig_band.format_flags();
    assert!(flags.is_empty(), "{flags:?}");

    let band = Band::open(&af, orig_band.id()).unwrap();
    println!("{band:?}");
    assert!(band.format_flags().is_empty());

    assert_eq!(band.band_format_version(), Some("0.6.3"));
    // TODO: When we do support some flags, check that the minimum version is 23.2.
}

#[test]
#[should_panic(expected = "unknown flag \"wibble\"")]
fn unknown_format_flag_panics_in_create() {
    let af = ScratchArchive::new();
    let _ = Band::create_with_flags(&af, &["wibble".into()]);
}

#[test]
fn unknown_format_flag_fails_to_open() {
    let af = ScratchArchive::new();

    // Make the bandhead by hand because the library prevents writing invalid flags.
    af.transport().create_dir("b0000").unwrap();
    let head = serde_json::json! ({
        "start_time": 1676651990,
        "band_format_version": "23.2.0",
        "format_flags": ["wibble"]
    });
    af.transport()
        .sub_transport("b0000")
        .write_file("BANDHEAD", &serde_json::to_vec(&head).unwrap())
        .unwrap();

    let err = Band::open(&af, &BandId::zero()).unwrap_err();
    println!("{err}");
    assert_matches!(err, Error::UnsupportedBandFormatFlags { .. });
    assert!(err
        .to_string()
        .starts_with(r#"Band b0000 has feature flags ["wibble"] not supported by Conserve "#));
}
