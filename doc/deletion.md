#  Deletion in Conserve

Conserve's formats are intended to perform well for a user who keeps making
backups without ever deleting data. In this case, files are never deleted or
rewritten. (Except that temporary files are written and then moved into place.)

However, some users may want to delete data from their archive, either to reduce
storage usage, or to remove data that should no longer be retained.

There are three types of deletion operation:

1. Deletion of versions/bands, including the band metadata and index
2. Deletion of stray temporary files
3. Deletion of unreferenced blocks

Deletion is an exception to Conserve's general promise to be safe if one task is
reading while another is writing, or even to allow multiple overlapping backups.

Deleting bands can break another client who is currently writing to them, but
they should break in a safe way, just seeing that the band is no longer present.

## Deletion guards

Garbage collection while a backup is underway could lead the backup process to
write a reference to a block which is imminently deleted by the gc process.

So we need to prevent new backups starting while gc is underway, and also
prevent gc starting while a backup is underway.

A pending backup can be detected by the gc task by the presence of an
incomplete band as the highest-numbered band. An incomplete band might be left
behind by an interrupted backup; the user can resolve this by running a new
backup that has time to complete, or by deleting the incomplete band.

A pending GC operation is marked by a `GC_LOCK` file in the root of the
archive. This might be left behind if the gc is interrupted, but the user can
run it again to allow it to complete.

Both these interlocks are managed by a `DeleteGuard` object.
