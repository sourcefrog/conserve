# Lease files

Lease files indicate that a process has exclusive write access to the archive. In particular, exclusive write access is needed during garbage collection.

When a process completes or is interrupted, it should release the lease. But, it's possible that the lease might not get correctly releaesed, for example if the client lost network connectivity to the archive.
