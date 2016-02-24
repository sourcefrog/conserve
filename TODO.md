* Write some files into a band.

* Validate names are clean
* Validate name ordering
* Add tests for unicode names

* recurse through source directory

* more validation (what?)

* compress index blocks

* Log compression ratio per block

* Detect and cope with incomplete bands

* Write multiple top-level bands so that you can make more than one backup to an archive

* list bands
* list files in a band, or in a version

* Use structured problem reporting rather than fatal glog (in all cases)
* Backup and restore directories, symlinks, etc (maybe also fifos, devices)
* Give a warning status if we kept going past problems
* Add -k option to keep going after problems

* Maybe put indexes in a separate directory for easier listing?

* Test handling of various broken archives - perhaps needs some scripts or infrastructure to construct them
 * bzip decompression failure
 * missing lower layers

* Write to temporary file and move into place?
  Actually needed, or is it better to just say that readers ought to cope
  with truncated files, which are likely to happen anyhow.

* Auto-resume incomplete bands

* Migrate from Cram to native Rust tests
