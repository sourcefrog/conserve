# Deletion in Conserve

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

Deleting unreferenced blocks or temporary files while a backup is underway
could potentially break the backup, by deleting data that it just wrote and
is soon going to reference from an index. This is prevented by a
`DeleteGuard` which aborts the deletion if there are any concurrent writes of
new bands.