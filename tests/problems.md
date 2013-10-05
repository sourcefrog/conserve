You get a clean error if the archive header is missing:

    $ mkdir a
    $ touch s
    $ conserve -L backup s a
    Archive head not found: is this an archive?
    Problem: archive.head.nonexistent: a/CONSERVE
    terminate called after throwing an instance of 'conserve::Problem const*'
    Aborted
    [134]
