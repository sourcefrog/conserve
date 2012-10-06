Dura is a (design for a) robust backup program
##############################################

Copyright 2012 Martin Pool.

Dura is licensed under the Apache License, Version 2.0.

At this time Dura is not ready for use.


Manifesto
*********

* A conservative and robust design that prioritizes recall of data from 
  backup.

* Simple transparent formats with limited dependencies.

  Even if there are bugs or failures in Dura or other systems, you should 
  have the best chance of getting at least some data back.

* Well matched for high-latency, limited-bandwidth, write-once cloud 
  storage.  Cloud storage typically doesn't have full filesystem semantics.

* Optional encryption on storage, using gpg.

* Backups should always make forward progress, even if they are never 
  given enough time to read the whole source or the whole destination.

* Restoring a subset of the backup must be reasonably fast, and not 
  require reading all history or the entire tree.

* Fast consistency checks of the backup that don't require reading
  all data back (which may be too slow to do regularly), because they
  can trust the storage is immutable and stable.

* Possibly-slow verification checks that actually do compare the backup
  to the source directory.

* Send backups to multiple locations: local disk, removable disk,
  LAN servers, the cloud. 

* A human oriented text UI, and a batch mode that can be read by 
  a GUI or other front end.

* Set up as a cron job or daemon and then no other maintenance is needed,
  other than sometimes manually double-checking the backups can be 
  restored and are complete.