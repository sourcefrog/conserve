Conserve 0.2.0 not released yet
===============================

* Rewrite in lovely Rust.
* Newly implmented commands:
  * `conserve init`: create an archive.  (Renamed from `init-archive`.)
  * `conserve backup`: copy a directory recursively into a new top-level
    version in the archive.  Incremental backups and exclusions are not yet
    supported.
  * `conserve list-source`: show what files are in the source directory and will
    potentially be backed up.
  * `conserve list-bands`: show what backups are in the archive.
* Changed format:
  * Metadata in json.
  * BLAKE2b hashes.
  * Brotli compression.
* `--stats` option shows how much IO was done.

Conserve 0.1.0 2013-10-01
=========================

* Very basic but functional backup, restore, and validate.
