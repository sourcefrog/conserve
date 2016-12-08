# Conserve: a robust backup program

[![Linux build status](https://travis-ci.org/sourcefrog/conserve.svg)](https://travis-ci.org/sourcefrog/conserve)
[![Windows build status](https://ci.appveyor.com/api/projects/status/uw61cgrek8ykfi7g?svg=true)](https://ci.appveyor.com/project/sourcefrog/conserve)
[![Join the chat at https://gitter.im/sourcefrog/conserve](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/sourcefrog/conserve?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

Conserve copies files, directories, and (on Unix) symlinks from a local *source*
tree, to an *archive* directory, and retrieves them on demand.

Conserve's [guiding principles](doc/manifesto.md):

* **Safe**: Conserve is written in [Rust][rust], a fast systems programming
   language with compile-time guarantees about types, memory safety, and
   concurrency.
* **Robust**:  If one file is corrupted in storage or due
   to a bug in Conserve, you can still restore others.
* **Careful**: Data files already written are never touched or altered,
   unless you choose to purge them.
* **When you need help now**: Restoring a subset of a large backup is fast.
* **Always ready**: You can restore recently-written files before the backup
   job completes.
* **Always making progress**: Even if the backup process or its network
   connection is repeatedly killed, Conserve can quickly pick up
   where it left off and make forward progress.
* **Ready for today**: The storage format is fast and reliable on on
   high-latency, limited-capability, unlimited-capacity, eventually-consistent
   cloud object storage.

## Quick start guide

    conserve init /backup/home.conserve
    conserve backup /backup/home.conserve ~
    conserve restore /backup/home.conserve /tmp/trial-restore

## Inspecting history

Conserve archives retain all previous versions of backups, stored in
*bands*.  Bands are identified a string of integers starting with `b`,
like `b0000`:

    $ conserve versions /backup/home.conserve
    b0000       2012-12-02T16:24:33   conservetesthost.local
    b0001       2012-12-02T16:24:45   conservetesthost.local

`ls` shows all the files in a band, including the
time they were made and the host from which they were made.
Like all commands that read a band from an archive, it operates
on the most recent by default.

## Install

Conserve runs on Linux, OS X, Windows, and probably other systems that
support Rust.

To build Conserve you need [Rust][rust] and a C compiler that can be used by
Rust.  Then run

    cargo build

[rust]: https://rust-lang.org/
[sourcefrog]: http://sourcefrog.net/

## More documentation

* [A comparison to other backup systems][comparison]

[comparison]: https://github.com/sourcefrog/conserve/wiki/Compared-to-others

* [Software and format versioning](doc/versioning.md)

* [Archive format](doc/format.md)

## Limitations

Conserve is still in a pre-1.0 alpha.  It can be used to make and restore
backups, but there are some important performance and functional limitations,
which will be fixed before 1.0.

* [Data compression is somewhat slow][32].
* [There are no incremental backups][41]: all backups store all files.
* [There is no way to exclude files/subdirectories from backup or restore][8].
* [You can only restore the most recent backup, not a named older one][42].
* The planned `validate` command is [not implemented][5],
  however a trial restore from the archive will test everything can be read.
* The planned feature of resuming an interrupted backup is not implemented:
  Conserve will just create a new full backup from the beginning.
* `conserve diff` is also not implemented, but can be simulated by restoring to
  a temporary directory and comparing that to the source.
* [The `conserve purge` command to trim the backup archive is not implemented][43],
  but the `b0123` band directories can be deleted directly.
* Permissions and ownership are not stored.

Prior to 1.0, data formats may change on each minor version number change (0.x):
you should restore using the same version that you used to make the backup.

[5]: https://github.com/sourcefrog/conserve/issues/5
[8]: https://github.com/sourcefrog/conserve/issues/8
[32]: https://github.com/sourcefrog/conserve/issues/32
[41]: https://github.com/sourcefrog/conserve/issues/41
[42]:https://github.com/sourcefrog/conserve/issues/42
[43]: https://github.com/sourcefrog/conserve/issues/43

For a longer list see the [issue tracker][issues] and 
[milestones][milestones].

[issues]: https://github.com/sourcefrog/conserve/issues
[milestones]: https://github.com/sourcefrog/conserve/milestones

## Licence and non-warranty

Copyright 2012-2016 [Martin Pool][sourcefrog], mbp@sourcefrog.net.

This program is free software; you can redistribute it and/or
modify it under the terms of the GNU General Public License
as published by the Free Software Foundation; either version 2
of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

## Contact

Conserve's homepage is: <http://conserve.fyi/> and you can talk
to me in [Gitter](https://gitter.im/sourcefrog/conserve).
