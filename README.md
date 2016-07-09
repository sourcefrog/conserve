# Conserve - a robust backup program

**At this time Conserve is not ready for production use.**

Copyright 2012-2016 [Martin Pool][sourcefrog], mbp@sourcefrog.net.

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

[![Clippy Linting Result](https://clippy.bashy.io/github/sourcefrog/conserve/master/badge.svg)](https://clippy.bashy.io/github/sourcefrog/conserve/master/log)

## Use

    conserve init /backup/my-source
    conserve backup ~/ /backup/my-source
    conserve validate /backup/my-source
    conserve restore /backup/my-source /tmp/source-restore

For more details see the
[`conserve(1)`](https://github.com/sourcefrog/conserve/blob/master/man/conserve.asciidoc)
man page.

## Install

Conserve runs on Unix, OS X, and Windows.

To build Conserve you need [Rust][rust] and a C compiler that can be used by
Rust.

Then simply run

    $ cargo build

[rust]: https://rust-lang.org/
[sourcefrog]: http://sourcefrog.net/

## More documentation

 * [Conserve Manifesto](doc/manifesto.md)

 * [A comparison to other backup systems](
   https://github.com/sourcefrog/conserve/wiki/Compared-to-others)

 * [Versioning](doc/versioning.md)

 * [Archive format](doc/format.md)
