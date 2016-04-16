Versioning
==========

Conserve software releases and formats are identified by the same `x.y.z`
version timeline.  

## Pre-1.0 releases

**Prior to 1.0**: Any snapshot of Conserve will
be able to read backups it has written, but **no guarantees** are made about
cross-version compatibility of formats, APIs, or commands.

## Archive format versions

The archive includes the version of Conserve that initialized it.

An archive with major version `x` can only be read by software with major
version `x`: this is to allow support for old features to be removed.  
This implies
that packages or installations of Conserve should allow concurrent installation
of multiple major versions.

Releases that differ in the minor version but not the major version, can make
forward-compatible changes in the format.  Anything written by x.y.z can be
read by x.yy.zz when yy>=y, and similarly for command lines.

Releases that differ only in the patchlevel, z, make no changes to the format
or command line interface: anything written by x.y.z can be read by x.y.zz for
any z, zz.  Any command line accepted by one will be accepted by the other.

## APIs

Post 1.0, Conserve's Rust API will be identical on patchlevels,
forward-compatible on minor versions, and there is no guarantee of compatibility
across major versions.

## Command line

Additions of new flags or commands to the command line interface will
be signaled by a new minor version.
