## Conserve 0.3.0

 * New commands:
   * `conserve restore`!
 * Symlinks are backed up and restored.
   (Only on Unix, they're skipped on Windows.)
 * Breaking archive format change:
   * Separate whole-file hash from the hash of the blocks making it up.
 * New text-mode progress bar.

## Conserve 0.2.0 (2016-04-18)

* Rewrite in lovely Rust.
* Newly implmented commands:
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

## Conserve 0.1.0 2013-10-01

* Very basic but functional backup, restore, and validate.
