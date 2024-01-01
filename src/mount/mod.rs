use std::path::Path;

use crate::{Archive, Result};

#[cfg(windows)]
mod projfs;

/// Options for mounting an archive
/// into an existing file systems.
pub struct MountOptions {
    /// Create the mount point and delete it
    /// when unmounting resulting in a clean environment.
    pub clean: bool,
}

pub fn mount(archive: Archive, destination: &Path, options: MountOptions) -> Result<()> {
    #[cfg(windows)]
    return projfs::mount(archive, destination, options);

    #[cfg(not(windows))]
    return Err(crate::Error::NotImplemented);
}
