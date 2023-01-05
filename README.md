# Conserve: a robust backup program

<https://github.com/sourcefrog/conserve/>

[![GitHub build status](https://github.com/sourcefrog/conserve/workflows/Rust/badge.svg?branch=main)](https://github.com/sourcefrog/conserve/actions?query=workflow%3ARust)
[![crates.io](https://img.shields.io/crates/v/conserve.svg)](https://crates.io/crates/conserve)
![Maturity: Beta](https://img.shields.io/badge/maturity-beta-yellow.svg)

<!-- [![Join the chat at https://gitter.im/sourcefrog/conserve](https://badges.gitter.im/Join%20Chat.svg)](https://gitter.im/sourcefrog/conserve?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge) -->

Conserve's [guiding principles](doc/manifesto.md):

- **Safe**: Conserve is written in [Rust][rust], a fast systems programming
  language with compile-time guarantees about types, memory safety, and
  concurrency. Conserve uses a
  [conservative log-structured format](doc/format.md).

- **Robust**: If one file is corrupted in storage or due to a bug in Conserve,
  or if the backup is interrupted, you can still restore what was written.
  (Conserve doesn't need a large transaction to complete for data to be
  accessible.)

- **Careful**: Backup data files are never touched or altered after they're
  written, unless you choose to purge them.

- **When you need help now**: Restoring a subset of a large backup is fast,
  because it doesn't require reading the whole backup.

- **Always making progress**: Even if the backup process or its network
  connection is repeatedly killed, Conserve can quickly pick up where it left
  off and make forward progress.

- **Ready for today**: The storage format is fast and reliable on on
  high-latency, limited-capability, unlimited-capacity, eventually-consistent
  cloud object storage.

- **Fast**: Conserve exploits Rust's _fearless concurrency_ to make full use of
  multiple cores and IO bandwidth. (In the current release there's still room to
  add more concurrency.)

- **Portable**: Conserve is tested on Windows, Linux (x86 and ARM), and OS X.

## Quick start guide

Conserve storage is within an _archive_ directory created by `conserve init`:

    $ conserve init /backup/home.cons

`conserve backup` copies a source directory into a new _version_ within the
archive. Conserve copies files, directories, and (on Unix) symlinks. If the
`conserve backup` command completes successfully (copying the whole source
tree), the backup is considered _complete_.

    $ conserve backup /backup/home.cons ~ --exclude /.cache

`conserve diff` shows what's different between an archive and a source
directory. It should typically be given the same `--exclude` options as were
used to make the backup.

    $ conserve diff /backup/home.cons ~ --exclude /.cache

`conserve versions` lists the versions in an archive, whether or not the backup
is _complete_, the time at which the backup started, and the time taken to
complete it. Each version is identified by a name starting with `b`.

    $ conserve versions /backup/home.cons
    b0000                      complete   2016-11-19T07:30:09+11:00     71s
    b0001                      incomplete 2016-11-20T06:26:46+11:00
    b0002                      incomplete 2016-11-20T06:30:45+11:00
    b0003                      complete   2016-11-20T06:42:13+11:00    286s
    b0004                      complete   2016-12-01T07:08:48+11:00     84s
    b0005                      complete   2016-12-18T02:43:59+11:00      4s

`conserve ls` shows all the files in a particular version. Like all commands
that read a band from an archive, it operates on the most recent by default, and
you can specify a different version using `-b`. (You can also omit leading zeros
from the backup version.)

    $ conserve ls -b b0 /backup/home.cons | less

`conserve restore` copies a version back out of an archive:

    $ conserve restore /backup/home.cons /tmp/trial-restore

`conserve validate` checks the integrity of an archive:

    $ conserve validate /backup/home.cons

## Exclusions

The `--exclude GLOB` option can be given to commands that operate on files,
including `backup`, `restore`, `ls` and `list-source`.

A `/` at the start of the exclusion pattern anchors it to the top of the backup
tree (not the root of the filesystem.) `**` recursively matches any number of
directories. `*.o` matches anywhere in the tree.

`--exclude-from` reads exclusion patterns from a file, one per line, ignoring
leading and trailing whitespace, and skipping comment lines that start with a
`#`.

The syntax is comes from the Rust [globset](https://docs.rs/globset/#syntax)
crate.

Directories marked with [`CACHEDIR.TAG`](https://bford.info/cachedir/) are
automatically excluded from backups.

## Install

To build Conserve you need [Rust][rust] and a C compiler that can be used by
Rust.

To install the most recent release from crates.io, run

    cargo install conserve

To install from a git checkout, run

    cargo install -f --path .

[rust]: https://rustup.rs/
[sourcefrog]: http://sourcefrog.net/

On nightly Rust only, you can enable a potential speed-up to the blake2 hashes
with

    cargo +nightly install -f --path . --features blake2_simd_asm

### Arch Linux

To install from from available
[AUR packages](https://aur.archlinux.org/packages/?O=0&SeB=nd&K=Robust+portable+backup+tool+written&outdated=&SB=n&SO=a&PP=50&do_Search=Go),
use an [AUR helper](https://wiki.archlinux.org/index.php/AUR_helpers):

    yay -S conserve

## More documentation

- [A comparison to other backup systems][comparison]

[comparison]: https://github.com/sourcefrog/conserve/wiki/Compared-to-others

- [Software and format versioning](doc/versioning.md)

- [Archive format](doc/format.md)

- [Design](doc/design.md)

- [Release notes](NEWS.md)

## Performance on Windows

Windows Defender and Windows Search Indexing can severely slow down any program that does intensive file IO, including Conserve. I recommend you exclude the backup directory from both systems.

## Project status

Conserve is at a reasonable level of maturity; the format is stable and the basic features are complete. I have used it as a primary backup system for over a year.

The current data format (called "0.6") will be readable by future releases for at least two years.

Be aware Conserve is developed as a part-time non-commercial project and there's no guarantee of support or reliability. Bug reports are welcome but I cannot promise they will receive a resolution within any particular time frame.

As of October 2022 I am primarily spending my open-source time on [cargo-mutants](https://github.com/sourcefrog/cargo-mutants). When that is feature complete, which is anticipated by early-mid 2023, I will likely come back to working more on Conserve.

There is still room for several performance improvements and features. See the [issue tracker][issues] for a list.

[5]: https://github.com/sourcefrog/conserve/issues/5
[8]: https://github.com/sourcefrog/conserve/issues/8
[32]: https://github.com/sourcefrog/conserve/issues/32
[41]: https://github.com/sourcefrog/conserve/issues/41
[42]: https://github.com/sourcefrog/conserve/issues/42
[43]: https://github.com/sourcefrog/conserve/issues/43

[issues]: https://github.com/sourcefrog/conserve/issues
[milestones]: https://github.com/sourcefrog/conserve/milestones

## Licence and non-warranty

Copyright 2012-2022 Martin Pool.

This program is free software; you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation; either version 2 of the License, or (at your option) any later
version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See the GNU General Public License for more details.
