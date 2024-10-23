use std::path::Path;

use crate::{Archive, Result, Error};
use super::{MountHandle, MountOptions};

pub fn mount(
    _archive: Archive,
    _destination: &Path,
    _options: MountOptions,
) -> Result<Box<dyn MountHandle>> {
    Err(Error::NotImplemented)
}
