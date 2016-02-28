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

Dependencies
============

Most dependencies will be automatically installed by `cargo build`.

To run the blackbox tests, [cram](https://pypi.python.org/pypi/cram) is needed:

    pip install cram

[1]: http://sourcefrog.net/
[2]: https://www.apache.org/licenses/LICENSE-2.0.html

More documentation
==================

 * [Conserve Manifesto](doc/manifesto.md)

 * [A comparison to other backup systems](
   https://github.com/sourcefrog/conserve/wiki/Compared-to-others)
   
 * [Versioning](doc/versioning.md)

 * [Archive format](doc/format.md)
