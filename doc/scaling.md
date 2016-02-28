Conserve scaling
============

Conceptual numbers: Source directory typically 1TB, stretch to 1PB,
the largest foreseeably likely to be on a single machine or linearly
readable in a reasonable amount of time.

Blocks perhaps 100MB each.  So that would be 10,000 blocks for 1TB, 10,000,000
blocks for 1PB.  (Perhaps if you're doing such a large backup you should make
a larger goal block size.)

Index directories have 10,000 files per subdirectory and with 10M blocks
there'll be 1000 directories in `i`.

Reasonably small number of tiers, less than ten: typically annual, monthly,
weekly, daily,  hourly.  That produces band directory names like
`b0000/b0000/b0000/b0000/b0000` which is reasonable.

Assume the storage must read a directory listing in a single operation.
(In practice, many can stream it?)  But, it may not be sorted.

Hourly backups, over 30 years = 262,000 bands.
List of bands does fit in memory: that would be maybe 262MB.

List of names of blocks in the active band also read from a single directory:
with 10k that is also about 1MB.  With a 1PB source that would be about 1GB of
names, which is perhaps stretching what you can store in a single Unix or
cloud directory?  Actual block indexes not all required in memory at one time.

Maybe 1e12 files?

On current protobuf implementation, the index is about 1/100th of the data
file, with both of them uncompressed.  With gzip, the data file is slightly more
compressible than the index; both compress about 3x.

No single source file needs to fit in memory.  List of source files does not
need to fit in memory.

Single blocks, and their index, must fit in memory.

Allowed in memory:
 - List of names of all blocks in one band.
 - Listing of any one source directory (to sort it).
 - List of all bands.
 - Index for any one block.

Not allowed to be required in memory:
 - Full contents of any source file.
 - All indexes for any band.
