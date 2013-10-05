Files are stored in order even if they're in a bad order on the command line.

    $ touch file1 file2
    $ conserve init myarchive
    $ conserve backup file2 file1 myarchive
    $ conserve -L validate myarchive
    Problem: entry.name.disordered: myarchive/b0000/a000000: file1 <= file2
    Terminating due to problem
    [3]

