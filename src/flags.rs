// Conserve backup system.
// Copyright 2015-2025 Martin Pool.

//! Per-band format flags.
//!
//! Strings stored in the band header name features that must be understood
//! by the Conserve version reading the band.

use serde::{
    de::{self, Unexpected, Visitor},
    Deserialize, Serialize,
};

/// A set of format marker flags.
#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FormatFlags {
    flags: Vec<Flag>,
}

impl FormatFlags {
    /// Return the default flags for writing new backups.
    ///
    /// This isn't called `default()` because it's not a semantically empty value;
    /// it indicates something about the behavior of this version of the library.
    pub fn current_default() -> FormatFlags {
        FormatFlags { flags: Vec::new() }
    }

    pub fn empty() -> FormatFlags {
        FormatFlags { flags: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Flag {}

impl<'de> Deserialize<'de> for Flag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(FlagVisitor)
    }
}

impl Serialize for Flag {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        unreachable!("Flag can't be instantiated")
    }
}

struct FlagVisitor;

impl<'de> Visitor<'de> for FlagVisitor {
    type Value = Flag;

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Err(de::Error::invalid_value(Unexpected::Str(v), &self))
    }

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "registered format flag name")
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
        assert_eq!(flags.flags, []);
        assert!(flags.is_empty());
        assert_eq!(format!("{flags:?}"), "FormatFlags { flags: [] }");
    }

    #[test]
    fn deserialize_empty_flags() {
        let flags: FormatFlags =
            serde_json::from_value(json! { [] }).expect("Failed to deserialize");
        assert!(flags.is_empty())
    }
}
