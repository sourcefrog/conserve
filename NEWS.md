# Conserve release history

## Conserve 0.4.1

* Large files are broken into multiple blocks of 1MB uncompressed content,
  so that memory use is capped and so that common blocks can potentially be
  shared.

## Conserve 0.4.0

* Switch from Brotli2 to Snappy compression: probably a better
  speed/size tradeoff for mixed data. (Breaks format compatibility.)

* Updated to work with Rust 1.22 and current library dependencies.

## Conserve 0.3.2

Released 2017-01-08.

* Flush (sync) archive files to stable storage after they're written.  In the
  event of the machine crashing or losing power in the middle of a
  backup, this should reduce the chance that there are index blocks
  pointing to data blocks not on the filesystem.

  Tests show this has little impact on performance and it's consistent with
  Conserve's value of safety.  (Windows 10 performance turns out to be ruined
  by the Windows Defender antivirus, but if you exclude the archive directory
  it is fine, even with this change.)

* New `--ui` option to choose plain text or fancy colored output, replacing
  `--no-progress`.

* Color UI shows progress bars cleanly interleaved with log messages.

* Filenames are now only shown during `backup` and `restore` when the `-v`
  option is given.

* `conserve versions` by default shows whether they're complete or not.
  `conserve versions --short` gives the same behavior as previously of
  just listing the version names.

* `conserve ls` and `conserve restore` will by default refuse to read
  incomplete versions, to prevent you thinking you restored the whole tree when
  it may be truncated.  You can override this with `--incomplete`, or select an
  older version with `--backup`.


## Conserve 0.3.1

Released 2016-12-17

* Fixed Cargo package metadata.

* New `--backup` option to `conserve ls` and `conserve restore` lets you
  retrieve older versions.

## Conserve 0.3.0

Released 2016-12-11

* Archive format has changed from 0.2 without backward compatibility.
* New and changed commands:
  * `conserve restore` makes Conserve a much more useful backup tool!
  * Renamed `list-versions` to just `versions`.
* Symlinks are backed up and restored.  (Only on Unix, they're skipped on
  Windows because they seem to be rare and to have complicated semantics.)
* New text-mode progress bar.
* Compression is substantially faster, through setting Brotli to level 4.

## Conserve 0.2.0

Released 2016-04-18

* Rewrite in lovely Rust.
* New commands:
  * `conserve init`: create an archive.  (Renamed from `init-archive`.)
  * `conserve backup`: copy a directory recursively into a new top-level
    version in the archive.  Incremental backups and exclusions are not yet
    supported.
  * `conserve list-source`: show what files are in the source directory and will
    potentially be backed up.
  * `conserve list-versions`: show what backups are in the archive.
  * `conserve ls`: lists files in the latest version in the archive.
* Changed format:
  * Metadata in json.
  * BLAKE2b hashes.
  * Brotli compression.
* `--stats` option shows how much IO was done, how much compression helped,
  and how much time was taken for various sub-operations.

## Conserve 0.1.0

Released 2013-10-01

* Very basic but functional backup, restore, and validate.
