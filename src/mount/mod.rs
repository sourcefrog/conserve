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

/// Handle for the mount controller.
/// Once dropped, the projection will be stopped and if specified so by MountOptions cleaned.
pub trait MountHandle {
    /// Returns the root path where the archive has been mounted.
    fn mount_root(&self) -> &Path;
}

#[cfg(windows)]
pub use projfs::mount;
    #[cfg(not(windows))]
pub fn mount(
    _archive: Archive,
    _destination: &Path,
    _options: MountOptions,
) -> Result<Box<dyn MountHandle>> {
    Err(crate::Error::NotImplemented)
}
