Dura is a (design for a) robust backup program
##############################################

Copyright 2012 Martin Pool.

Dura is licensed under the Apache License, Version 2.0.

At this time Dura is not ready for use.


Manifesto
*********

* The most important thing is that data be retrieved when it's needed;
  within reason it's worth trading off other attributes for that.

* The format should be robust: if some data is lost, it should still be
  possible to retrieve the rest of the tree.

* Simple formats and conservative internal design, to minimize the risk of
  loss due to internal bugs.

* Well matched for high-latency, limited-bandwidth, write-once cloud
  storage.  Cloud storage typically doesn't have full filesystem semantics, but is very unlikely to have IO errors.  But, also suitable
  for local online disk, removable storage, and remote (ssh) smart servers.

* Optional storage layers: compression (bzip2/gzip/lzo), encryption (gpg),
  redundancy (Reed-Solomon).

* Backups should always make forward progress, even if they are never
  given enough time to read the whole source or the whole destination.

* Restoring a single file or a subset of the backup must be reasonably
  fast, and must not require reading all history or the entire tree.

* Fast consistency checks of the backup that don't require reading
  all data back (which may be too slow to do regularly), because they
  can trust the storage is immutable and stable.

* Possibly-slow verification checks that actually do compare the backup
  to the source directory.

* Send backups to multiple locations: local disk, removable disk,
  LAN servers, the cloud.

* A human oriented text UI, and a machine UI that can be used to implement
  out-of-process UIs.  Web and GUI uis.

* Set up as a cron job or daemon and then no other maintenance is needed,
  other than sometimes manually double-checking the backups can be
  restored and are complete.

* The backup archive should be a pure function of the source directory
  and history of backup operations.  (If the backup metadata includes
  a timestamp, you can pass in the timestamp to get the same result.)
