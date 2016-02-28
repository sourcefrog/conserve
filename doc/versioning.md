Versioning
==========

Conserve will use the following versioning scheme:

Prior to 1.0: formats are not guaranteed stable. Any snapshot of Conserve will
be able to read backups it has written, but backward or forward compatibility
may break.

From 1.0 onwards, Conserve will use a three-element version, _x.y.z_, applying
[semantic versioning](http://semver.org/) principles to archive formats and
command lines:

Releases that differ only in the patchlevel, z, make no changes to the format
or command line interface: anything written by x.y.z can be read by x.y.zz for
any z, zz.  Any command line accepted by one will be accepted by the other.

Releases that differ in the minor version but not the major version, may make
forward-compatible changes in the format.  Anything written by x.y.z can be
read by x.yy.zz when yy>y, and similarly for command lines.  The minor version
may also be incremented for major but not compatibility-breaking changes.

Releases that differ in the major version may not be able to read archives
written by previous versions.
