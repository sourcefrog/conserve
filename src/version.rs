/// Conserve version number as a semver string.
///
/// This is populated at compile time from `Cargo.toml`.
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
