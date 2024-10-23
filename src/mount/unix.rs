use std::path::Path;

use super::{MountHandle, MountOptions};
use crate::{Archive, Error, Result};

pub fn mount(
    _archive: Archive,
    _destination: &Path,
    _options: MountOptions,
) -> Result<Box<dyn MountHandle>> {
    Err(Error::NotImplemented)
}
