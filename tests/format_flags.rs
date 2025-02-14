// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

//! Tests for per-band format flags.

use conserve::*;
use transport::WriteMode;
#[tokio::test]
async fn unknown_format_flag_fails_to_open() {
    let af = Archive::create_temp().await;

    // Make the bandhead by hand because the library prevents writing invalid flags.
    af.transport().create_dir("b0000").unwrap();
    let head = serde_json::json! ({
        "start_time": 1676651990,
        "band_format_version": "23.2.0",
        "format_flags": ["wibble"]
    });
    af.transport()
        .chdir("b0000")
        .write(
            "BANDHEAD",
            &serde_json::to_vec(&head).unwrap(),
            WriteMode::CreateNew,
        )
        .unwrap();

    let err = Band::open(&af, BandId::zero()).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "Unsupported band format flags [\"wibble\"] in b0000"
    )
}
