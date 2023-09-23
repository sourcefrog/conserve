# damage tests

Conserve tries to still allow the archive to be read, and future backups to be written,
even if some files are damaged: truncated, corrupt, missing, or unreadable.

This is not yet achieved in every case, but the format and code are designed to
work towards this goal.

These API tests write an archive, create some damage, and then try to read other
information, write future backups, and validate.

These are implemented as API tests for the sake of execution speed and ease of examining the results.

"Damage strategies" are a combination of a "damage action" (which could be deleting or
truncating a file) and a "damage location" which selects the file to damage.
