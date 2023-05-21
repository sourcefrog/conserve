# damage tests

Conserve tries to still allow the archive to be read, and future backups to be written,
even if some files are damaged: truncated, corrupt, missing, or unreadable.

This is not yet achieved in every case, but the format and code are designed to
work towards this goal.

These API tests write an archive, create some damage, and then try to read other
information, write future backups, and validate.
