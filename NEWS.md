# Conserve release history

## v0.6.6 2020-08-30

### Performance

- `validate` and other operations that list all blocks no longer `lstat` them,
  which can be a significant performance improvement on some kernels or
  filesystems.

### Features

- Better progress bars, especially for `validate`, including an estimated time
  to completion.

### Bug fixes

- Remove needlessly-alarming warning about empty index hunks.

## v0.6.5 2020-07-26

### Features

- New `conserve debug unreferenced ARCHIVE` lists unreferenced blocks.

- Conserve now "stitches" together incomplete backups with the previous index.
  This means that restoring from a backup that did not complete will give the
  most complete available copy of the tree at that point in time.

### Behavior changes

- The `--incomplete` option, to read the partial tree from an interrupted
  backup, is no longer needed and has been removed.

## v0.6.4 2020-07-04

### Features

- New `conserve restore --only SUBTREE` option restores only one subtree of the
  archive. Thanks to Francesco Gadaleta.

### Performance improvements

`conserve validate` is now significantly faster. Conserve remembers which blocks
have been validated and what their uncompressed length is, and uses this when
checking that file index entries are valid.

### Behavior changes

Some rearrangements to the command-line grammar to make it more concise and
consistent:

- `conserve debug` commands are now briefer: `conserve debug index`,
  `conserve debug referenced`, `conserve debug blocks`.

- `conserve source ls DIR` is now `conserve ls --source DIR`.

- `conserve source size DIR` is now `conserve size --source DIR`.

- `conserve tree size` is now `conserve size`.

- Obsolete global option `--ui` was removed.

- The short option for `conserve versions --short` is now `-q`.

- The short option for `conserve versions --sizes` is now `-z`.

### Internal

- Change to using [structopt](https://docs.rs/structopt/) for option parsing.

- Change to using [thiserror](https://docs.rs/thiserror/) for error enum
  construction.

- Added a new `Transport` trait, abstracting the IO options for the Archive so
  that Conserve can in future also access archives over SFTP or in cloud
  storage.

- Add tests that Conserve can read and write archives written by older
  compatible versions. (At present, everything from the 0.6 series that changed
  the format.)

- New simpler `Archive::backup` and `Archive::restore` public APIs.

### Archive format changes

Conserve 0.6.4 uses the same 0.6 archive version, with one compatible addition:

- BANDTAIL files contain an additional `index_hunk_count` to enable
  `conserve validate` to check that no hunks are missing.

## v0.6.3 2020-05-30

### Performance improvements

- Improved performance of incremental backups, by removing check that blocks
  referenced by the previous backup are still present. In one experiment of
  writing a large tree with few changes to a moderately-slow USB drive, this
  cuts overall elapsed time by a factor of about 7x!

  The check is redundant unless the archive has somehow been corrupted: blocks
  are always written out before the index hunk than references them.

  The basic approach in Conserve is to assume, in commands other than
  `validate`, that the archive is correctly formatted, and to avoid unnecessary
  ad-hoc checks that this is true.

### Behavior changes

- Removed global `--stats` option. Stats are always shown as info-level
  messages.

- Better ISO 8601 style timestamps in `conserve versions` output.

### Bugs fixed

- Don't panic on timestamps on or before the Unix epoch in 1970. (#100)

- Correctly count index IO in the backup stats summary. (#87)

### Documentation improved

- Improved, updated, and corrected format and design documentation (in the `doc`
  subdirectory of the source tree.)

### Archive format changes

- Conserve 0.6.3 uses the same 0.6 archive format, but backups it writes can
  only be read by 0.6.3 and later.

- Add a per-band minimum version (`BAND_FORMAT_VERSION`), allowing for future
  additions to the format without requiring a whole new archive. This is stored
  in `band_format_version` within the `BANDHEAD` file. Conserve gives a clean
  error message if it can't read the per-band minimum version. (#96)

- Improved index uncompressed size slightly, by omitting the data offset within
  the block when it is zero, which is common.

### API and internal changes

Various, including:

- Removal of `Report` concept. Instead, operations return a type-specific
  `Stats` describing the work that was done, messages are logged, and progress
  bars are drawn through the `ui` module.

- New small code style guide.

## Conserve 0.6.2 2020-02-06

- Added nanosecond precision to stored mtimes. The main benefit of this is
  more-precise detection of files that changed less than a second after the
  previous backup.

- Changed `conserve tree size` and `conserve source size` to report in MB, not
  bytes, as used elsewhere in the UI.

- Improved the speed of source tree iteration, and therefore the speed of
  backups with many unchanged files.

- Add back `conserve versions --sizes`, but showing the size of the tree in each
  version.

- Improved performance of backup.

## Conserve 0.6.1 2020-01-25

- Improved performance on incremental backup, by only opening source files if we
  need to read the contents.

## Conserve 0.6.0 2020-01-20

- Changed to new archive format "0.6", which has common block storage across
  bands, and removes the whole-file hash in favor of per-block hashes.

  To read from Conserve 0.5 archives, use an old Conserve binary. Until 1.0,
  support for old formats won't be kept in the head version.

- Added incremental backups! If files have the same size and mtime (tracked with
  integer second accuracy), they aren't read and stored but rather a reference
  to the previous block is added.

- Added a basic `conserve diff` command, which compares a source directory to a
  stored tree.

- Changed to Rust edition 2018.

- Added command `conserve debug index dump`.

- Removed `conserve versions --sizes` options, as storage is now shared across
  bands. The size of one stored tree can be measured with `conserve tree size`.

## Conserve 0.5.1 2018-11-11

- `conserve validate` checks the archive much more thoroughly.

- New `source size` and `tree size` commands.

- Progress percentage is now measured as a fraction of the total tree to be
  copied, which is a more linear measurement.

- Removed internal timing of operations, shown in `--stats`. Now that Conserve
  is increasingly aggressively multithreaded, these times aren't very
  meaningful, and the implementation causes some lock contention.

## Conserve 0.5.0 2018-11-01

- Conserve 0.5 uses a new format, and can't read 0.4 repositories. The new
  format has a single blockdir per archive for all file contents, rather than
  one per band. This significantly reduces space usage and backup time.

- New command `validate` checks some (but not yet all) internal correctness and
  consistency properties of an archive.

- New commands `conserve debug block list` and
  `conserve debug block referenced`.

- `conserve list-source` was renamed to `conserve source ls`.

- Better progress bars including percentage completion for many operations.

- `backup`, `restore`, and `validate` show a summary of what they did.

## Conserve 0.4.3 2018-10-13

- `conserve versions` has a new `--sizes` option, to show disk usage by each
  version.

- `-v` option to `backup` and `restore` prints filenames as they're processed.
  `--no-progress` turns off the progress bar.

## Conserve 0.4.2 2018-01-18

- Commands such as `restore` and `ls` that operate on a version, will by default
  operate on the last complete version, rather than defaulting to the last
  version altogether and then potentially complaining it's incomplete. Similarly
  for the `SourceTree::open` API when given no `BandId` argument.

- Some backup work is parallelized using Rayon, giving a mild speedup for large
  files. There is potential to much more work here, because backups are
  generally CPU-bound in Snap compression and BLAKE2 hashing, and Conserve
  should try to use every available core.

- Various internal rearrangements including treating stored and live trees as
  instances of a common trait, to enable future features.

## Conserve 0.4.1

- Large files are broken into multiple blocks of 1MB uncompressed content, so
  that memory use is capped and so that common blocks can potentially be shared.

- New `--exclude GLOB` option.

## Conserve 0.4.0

- Switch from Brotli2 to Snappy compression: probably a better speed/size
  tradeoff for mixed data. (Breaks format compatibility.)

- Updated to work with Rust 1.22 and current library dependencies.

## Conserve 0.3.2

Released 2017-01-08.

- Flush (sync) archive files to stable storage after they're written. In the
  event of the machine crashing or losing power in the middle of a backup, this
  should reduce the chance that there are index blocks pointing to data blocks
  not on the filesystem.

  Tests show this has little impact on performance and it's consistent with
  Conserve's value of safety. (Windows 10 performance turns out to be ruined by
  the Windows Defender antivirus, but if you exclude the archive directory it is
  fine, even with this change.)

- New `--ui` option to choose plain text or fancy colored output, replacing
  `--no-progress`.

- Color UI shows progress bars cleanly interleaved with log messages.

- Filenames are now only shown during `backup` and `restore` when the `-v`
  option is given.

- `conserve versions` by default shows whether they're complete or not.
  `conserve versions --short` gives the same behavior as previously of just
  listing the version names.

- `conserve ls` and `conserve restore` will by default refuse to read incomplete
  versions, to prevent you thinking you restored the whole tree when it may be
  truncated. You can override this with `--incomplete`, or select an older
  version with `--backup`.

## Conserve 0.3.1

Released 2016-12-17

- Fixed Cargo package metadata.

- New `--backup` option to `conserve ls` and `conserve restore` lets you
  retrieve older versions.

## Conserve 0.3.0

Released 2016-12-11

- Archive format has changed from 0.2 without backward compatibility.
- New and changed commands:
  - `conserve restore` makes Conserve a much more useful backup tool!
  - Renamed `list-versions` to just `versions`.
- Symlinks are backed up and restored. (Only on Unix, they're skipped on Windows
  because they seem to be rare and to have complicated semantics.)
- New text-mode progress bar.
- Compression is substantially faster, through setting Brotli to level 4.

## Conserve 0.2.0

Released 2016-04-18

- Rewrite in lovely Rust.
- New commands:
  - `conserve init`: create an archive. (Renamed from `init-archive`.)
  - `conserve backup`: copy a directory recursively into a new top-level version
    in the archive. Incremental backups and exclusions are not yet supported.
  - `conserve list-source`: show what files are in the source directory and will
    potentially be backed up.
  - `conserve list-versions`: show what backups are in the archive.
  - `conserve ls`: lists files in the latest version in the archive.
- Changed format:
  - Metadata in json.
  - BLAKE2b hashes.
  - Brotli compression.
- `--stats` option shows how much IO was done, how much compression helped, and
  how much time was taken for various sub-operations.

## Conserve 0.1.0

Released 2013-10-01

- Very basic but functional backup, restore, and validate.
