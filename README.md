# Conserve: a robust backup program

<http://conserve.fyi>

[![Linux build status](https://travis-ci.org/sourcefrog/conserve.svg?branch=master)](https://travis-ci.org/sourcefrog/conserve)
[![Windows build status](https://ci.appveyor.com/api/projects/status/uw61cgrek8ykfi7g?svg=true)](https://ci.appveyor.com/project/sourcefrog/conserve)
[![crates.io](https://img.shields.io/crates/v/conserve.svg)](https://crates.io/crates/conserve)
[![Join the chat at https://gitter.im/sourcefrog/conserve](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/sourcefrog/conserve?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

Conserve's [guiding principles](doc/manifesto.md):

* **Safe**: Conserve is written in [Rust][rust], a fast systems programming
  language with compile-time guarantees about types, memory safety, and
  concurrency.
  Conserve uses a [conservative log-structured format](doc/format.md).

* **Robust**:  If one file is corrupted in storage or due
  to a bug in Conserve, or if the backup is interrupted, you can still
  restore what was written.  (Conserve doesn't need a large transaction to
  complete for data to be accessible.)

* **Careful**: Backup data files are never touched or altered after they're
  written, unless you choose to purge them.

* **When you need help now**: Restoring a subset of a large backup is fast.

* **Always making progress**: Even if the backup process or its network
  connection is repeatedly killed, Conserve can quickly pick up
  where it left off and make forward progress.

* **Ready for today**: The storage format is fast and reliable on on
  high-latency, limited-capability, unlimited-capacity, eventually-consistent
  cloud object storage.  Conserve is tested on Windows, Linux (x86 and ARM),
  and OS X.


## Quick start guide

Conserve storage is within an *archive* directory created by `conserve init`:

    conserve init /backup/home.cons

`conserve backup` copies a source directory into a new *version* within the archive.
Conserve copies files, directories, and (on Unix) symlinks.
If the `conserve backup` command completes successfully (copying the whole
source tree), the backup is considered *complete*.

    conserve backup /backup/home.cons ~

Use `--exclude GLOB` to exclude files or directories from the backup using
[glob patterns](https://docs.rs/globset/0.2.1/globset/#syntax).

`conserve versions` lists the versions in an archive,
whether or not the backup is *complete*,
the time at which the backup started,
and the time taken to complete it.
Each version is identified by a name starting with `b`.

    $ conserve versions /backup/home.cons
    b0000                      complete   2016-11-19T07:30:09+11:00     71s
    b0001                      incomplete 2016-11-20T06:26:46+11:00
    b0002                      incomplete 2016-11-20T06:30:45+11:00
    b0003                      complete   2016-11-20T06:42:13+11:00    286s
    b0004                      complete   2016-12-01T07:08:48+11:00     84s
    b0005                      complete   2016-12-18T02:43:59+11:00      4s

`conserve ls` shows all the files in a particular version.  Like all commands
that read a band from an archive, it operates on the most recent by default, and
you can specify a different version using `-b`.  (You can also omit leading zeros
from the backup version.)

    $ conserve ls -b b0 /backup/home.cons | less


`conserve restore` copies a version back out of an archive.

    $ conserve restore /backup/home.cons /tmp/trial-restore

## Exclusions

The `--exclude GLOB` option can be given to commands that operate on files,
including `backup`, `restore`, `ls` and `list-source`.

A `/` at the start of the exclusion pattern anchors it to the top of the backup
tree (not the root of the filesystem.)  `**` recursively matches any number
of directories.

## Install

To build Conserve you need [Rust][rust] and a C compiler that can be used by
Rust.  Then run

    cargo build

Binaries for some platforms are available from
<https://github.com/sourcefrog/conserve/releases>.

[rust]: https://rust-lang.org/
[sourcefrog]: http://sourcefrog.net/

On nightly Rust only, you can enable a potential speed-up to the blake2 hashes with

    rustup run nightly cargo build --release --features blake2-rfc/simd_asm

## More documentation

* [A comparison to other backup systems][comparison]

[comparison]: https://github.com/sourcefrog/conserve/wiki/Compared-to-others

* [Software and format versioning](doc/versioning.md)

* [Archive format](doc/format.md)

* [Release notes](NEWS.md)

* [API docs](https://docs.rs/conserve/)


## Limitations

Conserve is still in a pre-1.0 alpha.  It can be used to make and restore
backups, but there are some important performance and functional limitations,
which will be fixed before 1.0.

* [There are no incremental backups][41]: all backups store all files.
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

Windows Defender and Windows Search Indexing can slow the system down severely
when Conserve is making a backup.  I recommend you exclude the backup directory
from both systems.


## Licence and non-warranty

Copyright 2012-2017 [Martin Pool][sourcefrog], mbp@sourcefrog.net.

This program is free software; you can redistribute it and/or
modify it under the terms of the GNU General Public License
as published by the Free Software Foundation; either version 2
of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.
