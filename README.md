# Conserve - a robust backup program

**At this time Conserve can make and restore backups, but should
not be relied upon for production use:
[more details](#Shortcomings).**

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

[![Linux build status](https://travis-ci.org/sourcefrog/conserve.svg)](https://travis-ci.org/sourcefrog/conserve)
[![Windows build status](https://ci.appveyor.com/api/projects/status/uw61cgrek8ykfi7g?svg=true)](https://ci.appveyor.com/project/sourcefrog/conserve)

[![Join the chat at https://gitter.im/sourcefrog/conserve](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/sourcefrog/conserve?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

## Use

    conserve init /backup/home.conserve
    conserve backup /backup/home.conserve ~
    conserve restore /backup/home.conserve /tmp/source-restore
    conserve --help

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

 * [Software and format versioning](doc/versioning.md)

 * [Archive format](doc/format.md)


## Shortcomings

Conserve is still in a pre-1.0 alpha.  It can be used to make and restore
backups, but there are some important performance and functional limitations,
which will be fixed before 1.0.

* There is no guarantee or testing of [forward and backward format
  compatibility](doc/versioning.md):
  you should restore using the same Conserve version that wrote
  the backup.
* [Data compression is slow][32].
* Backup data should be packed into medium-sized files, but at present
  archives may contain too many small data files.
* There are no incremental backups: all backups store all files.
* [There is no way to exclude files/subdirectories from backup or restore][8].
* [The planned `validate` command is not implemented][5],
  however a trial restore from the archive will .
* The planned feature of resuming an interrupted backup is not implemented:
  Conserve will just create a new full backup from the beginning.
* `conserve diff` is also not implemented, but can be simulated by restoring to
  a temporary directory and comparing that to the source.
* The `conserve cull` command to trim the backup archive is not implemented,
  but the `b0123` band directories can be deleted directly.
* You can only restore the most recent backup, not a named older one.
* Permissions and ownership are not stored.

[5]: https://github.com/sourcefrog/conserve/issues/5
[8]: https://github.com/sourcefrog/conserve/issues/8
[32]: https://github.com/sourcefrog/conserve/issues/32

For a longer list see [TODO](https://github.com/sourcefrog/conserve/wiki/TODO)
in the wiki.
