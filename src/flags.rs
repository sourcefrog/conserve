// Conserve backup system.
// Copyright 2015-2025 Martin Pool.

//! Per-band format flags.
//!
//! Strings stored in the band header name features that must be understood
//! by the Conserve version reading the band.

use std::borrow::Cow;

/// Default flags for newly created bands.
pub static DEFAULT: &[Cow<'static, str>] = &[];

pub static SUPPORTED: &[&str] = &[];
