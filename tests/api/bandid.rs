// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

use std::str::FromStr;

use assert_matches::assert_matches;

use conserve::{BandId, Error};

#[test]
#[should_panic]
fn empty_id_not_allowed() {
    BandId::new(&[]);
}

#[test]
fn equality() {
    assert_eq!(BandId::new(&[1]), BandId::new(&[1]))
}

#[test]
fn zero() {
    assert_eq!(BandId::zero().to_string(), "b0000");
}

#[test]
fn zero_has_no_previous() {
    assert_eq!(BandId::zero().previous(), None);
}

#[test]
fn previous_of_one_is_zero() {
    assert_eq!(
        BandId::zero().next_sibling().previous(),
        Some(BandId::zero())
    );
}

#[test]
fn next_of_zero_is_one() {
    assert_eq!(BandId::zero().next_sibling().to_string(), "b0001");
}

#[test]
fn next_of_two_is_three() {
    assert_eq!(BandId::from(2).next_sibling().to_string(), "b0003");
}

#[test]
fn to_string() {
    let band_id = BandId::new(&[20]);
    assert_eq!(band_id.to_string(), "b0020");
}

#[test]
fn large_value_to_string() {
    assert_eq!(BandId::new(&[2_000_000]).to_string(), "b2000000")
}

#[test]
fn from_string_detects_invalid() {
    assert!(BandId::from_str("").is_err());
    assert!(BandId::from_str("hello").is_err());
    assert!(BandId::from_str("b").is_err());
    assert!(BandId::from_str("b-").is_err());
    assert!(BandId::from_str("b2-").is_err());
    assert!(BandId::from_str("b-2").is_err());
    assert!(BandId::from_str("b2-1-").is_err());
    assert!(BandId::from_str("b2--1").is_err());
    assert!(BandId::from_str("beta").is_err());
    assert!(BandId::from_str("b-eta").is_err());
    assert!(BandId::from_str("b-1eta").is_err());
    assert!(BandId::from_str("b-1-eta").is_err());
}

#[test]
fn from_string_valid() {
    assert_eq!(BandId::from_str("b0001").unwrap().to_string(), "b0001");
    assert_eq!(BandId::from_str("b123456").unwrap().to_string(), "b123456");
}

#[test]
fn dashes_are_no_longer_valid() {
    // Versions prior to 23.2 accepted bandids with dashes, but never
    // used them.
    let err = BandId::from_str("b0001-0100-0234").unwrap_err();
    assert_matches!(err, Error::InvalidVersion { .. });
}

#[test]
fn to_string_respects_padding() {
    let s = format!("{:<10}", BandId::from(42));
    assert_eq!(s.len(), 10);
    assert_eq!(s, "b0042     ");
}
