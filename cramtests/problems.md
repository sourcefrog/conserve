You get a clean error if the archive header is missing:

    $ mkdir a
    $ touch s
    $ conserve -L backup s a
    Problem: archive.head.missing: a/CONSERVE: No such file or directory
    Terminating due to problem
    [3]
