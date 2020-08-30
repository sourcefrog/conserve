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

Deleting unreferenced blocks or temporary files while a backup is underway could
break the backup, by deleting data that it just wrote and is soon going to
reference from an index.

## Deletion guards

As a safety measure therefore, Conserve won't delete blocks or temporary files
if a backup is pending, or if a backup is made between identifying the files to
delete, and actually deleting them.

Before starting to collect a list of unreferenced files or temporary files,
Conserve checks the id of the last band, and checks whether it's closed. If it's
not closed, this is immediately an error.

Conserve then collects a list of files to delete, in particular by finding all 
present blocks and subtracting all referenced blocks.

Before actually begining deletion, Conserve then checks the last version is the
same as previously observed.
