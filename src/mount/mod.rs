use std::path::Path;

#[cfg(windows)]
mod projfs;

#[cfg(unix)]
mod unix;

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

#[cfg(unix)]
pub use unix::mount;
