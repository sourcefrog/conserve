// Conserve backup system.
// Copyright 2015-2025 Martin Pool.

//! Per-band format flags.
//!
//! Strings stored in the band header name features that must be understood
//! by the Conserve version reading the band.

use serde::{Deserialize, Serialize};

use crate::{BandId, Error, Result};

pub const SUPPORTED: &[&str] = &[];

/// A set of format marker flags.
///
/// It's legal to deserialize flags that are not supported by this version of the library.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FormatFlags {
    flags: Vec<String>,
}

impl FormatFlags {
    /// Return the default flags for writing new backups.
    ///
    /// This isn't called `default()` because it's not a semantically empty value;
    /// it indicates something about the behavior of this version of the library.
    pub fn current_default() -> FormatFlags {
        FormatFlags {
            flags: SUPPORTED.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn empty() -> FormatFlags {
        FormatFlags { flags: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    pub fn check_supported(&self, band_id: BandId) -> Result<()> {
        let mut unsupported_flags = Vec::new();
        for flag in &self.flags {
            if !SUPPORTED.contains(&flag.as_str()) {
                unsupported_flags.push(flag.clone());
            }
        }
        if unsupported_flags.is_empty() {
            Ok(())
        } else {
            Err(Error::UnsupportedBandFormatFlags {
                band_id,
                unsupported_flags,
            })
        }
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;

    #[test]
    fn default_flags_are_empty() {}

    #[test]
    fn current_default_flags_are_empty() {
        let flags = FormatFlags::current_default();
        assert_eq!(flags.flags, [] as [String; 0]);
        assert!(flags.is_empty());
        assert_eq!(format!("{flags:?}"), "FormatFlags { flags: [] }");
    }

    #[test]
    fn deserialize_empty_flags() {
        let flags: FormatFlags =
            serde_json::from_value(json! { [] }).expect("Failed to deserialize");
        assert!(flags.is_empty())
    }

    #[test]
    fn deserialize_unsupported_flags() {
        let flags: FormatFlags =
            serde_json::from_value(json! { ["garbage_salad"] }).expect("Failed to deserialize");
        assert!(!flags.is_empty());
        assert_eq!(flags.flags, ["garbage_salad"]);
        let err = flags.check_supported(BandId::zero()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Unsupported band format flags [\"garbage_salad\"] in b0000"
        )
    }
}
