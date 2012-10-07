Dura compared to other programs
###############################

Dura fills a good niche of being simpler and safer than Duplicity or Obnam, but
more friendly to cloud storage than rsync.

The great thing about backup programs is, to some extent, you don't have to
pick just one: for safety, make multiple backups with different tools and
approaches.  So this document is not to knock the other tools or to persuade
you not to use them, but rather to explain why it's worth writing Dura too.

Duplicity
*********

Duplicity addresses a similar case of backup to cloud or dumb servers, and
uses librsync delta compression.  But it fails several Dura manifesto items,
some of them by design:

* Verification and restoring a single file requires reading the whole archive

* I have experienced Duplicity bugs that make the archive unreadable and,
  although it is based on simpler underlying formats (rdiff, rdiffdir, tar, etc)
  it is still hard in practice to recover

* Performance is unpredictable and sometimes bad.

Obnam
*****


obnam: format too complex? Otherwise looks pretty good.
