Conserve - a robust backup program
==================================

**At this time Conserve is not ready for production use.**

Copyright 2012-2016 [Martin Pool][1], mbp@sourcefrog.net.

_This program is free software; you can redistribute it and/or
modify it under the terms of the GNU General Public License
as published by the Free Software Foundation; either version 2
of the License, or (at your option) any later version._

_This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details._

Conserve's homepage is: <https://github.com/sourcefrog/conserve>

[![Build Status](https://travis-ci.org/sourcefrog/conserve.png?branch=rust)](https://travis-ci.org/sourcefrog/conserve)

Use
---

This doesn't work yet:

    conserve init /backup/my-source
    conserve backup ~/ /backup/my-source
    conserve validate /backup/my-source
    conserve restore /backup/my-source /tmp/source-restore

For more details see the
[`conserve(1)`](https://github.com/sourcefrog/conserve/blob/master/man/conserve.asciidoc)
man page.

For a description of the approach and goals, see the
[Conserve Manifesto](doc/manifesto.md).

For a comparison to other backup systems, see
<https://github.com/sourcefrog/conserve/wiki/Compared-to-others>.


Dependencies
============

Most dependencies will be automatically installed by `cargo build`.

To run the blackbox tests, [cram](https://pypi.python.org/pypi/cram) is needed:

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
