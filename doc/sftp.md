# SFTP support

Conserve can read and write archives over SFTP.

To use this, just specify an SFTP URL, like `sftp://user@host/path`, for the archive location.

    conserve init sftp://user@host/path
    conserve backup sftp://user@host/path ~

If no username is present in the URL, Conserve will use the current user's username.

Currently, Conserve only supports agent authentication.
