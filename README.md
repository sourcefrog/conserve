Conserve is a (design for a) robust backup program
==============================================

**At this time Conserve is not ready for use.**

Copyright 2012-2013 [Martin Pool][1], mbp@sourcefrog.net.

_This program is free software; you can redistribute it and/or
modify it under the terms of the GNU General Public License
as published by the Free Software Foundation; either version 2
of the License, or (at your option) any later version._

_This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details._

Conserve's homepage is: <https://github.com/sourcefrog/conserve>


Manifesto
---------

* The most important thing is that data be retrieved when it's needed;
  within reason it's worth trading off other attributes for that.

* The format should be robust: if some data is lost, it should still be
  possible to retrieve the rest of the tree.

* Use simple formats and conservative internal design, to minimize the risk of
  loss due to internal bugs.

* Well matched for high-latency, limited-bandwidth, write-once cloud
  storage.  Cloud storage typically doesn't have full filesystem semantics,
  but is very unlikely to have IO errors.  Conserve is also suitable
  for local online disk, removable storage, and remote (ssh) smart servers.

* Optional storage layers: compression (bzip2/gzip/lzo), encryption (gpg),
  redundancy (Reed-Solomon).

* Backups should always make forward progress, even if they are never
  given enough time to read the whole source or the whole destination.

* Restoring a single file or a subset of the backup must be reasonably
  fast, and must not require reading all history or the entire tree.

* Provide and encourage fast consistency checks of the backup that
  don't require reading all data back (which may be too slow to do regularly).

* Also, possibly-slow verification checks that actually do compare the backup
  to the source directory, to catch corruption or Conserve bugs.

* Send backups to multiple locations: local disk, removable disk,
  LAN servers, the cloud.

* A human oriented text UI, and a machine UI that can be used to implement
  out-of-process UIs.  Web and GUI uis.

* Set up as a cron job or daemon and then no other maintenance is needed,
  other than sometimes manually double-checking the backups can be
  restored and are complete.

* The backup archive should be a pure function of the source directory
  and history of backup operations.  (If the backup metadata includes
  a timestamp, you can pass in the timestamp to get the same result.)


Dependencies
============

Ubuntu/Debian package names:

    libprotobuf-dev
    clang
    protobuf-compiler
    make
    libgoogle-glog-dev

To run the tests, [cram](https://pypi.python.org/pypi/cram) is needed:

    pip install cram

[1]: http://sourcefrog.net/
[2]: https://www.apache.org/licenses/LICENSE-2.0.html

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
