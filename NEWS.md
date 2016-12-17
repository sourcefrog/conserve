# Conserve release history

## Conserve 0.3.2

Not released yet.

## Conserve 0.3.1

Released 2016-12-17

* Fixed Cargo package metadata.

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
