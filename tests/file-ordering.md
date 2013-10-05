Files are stored in order even if they're in a bad order on the command line.

    $ touch file1 file2
    $ conserve init myarchive
    
TODO: conserve backup file2 file1 myarchive
currently fails because we check for this but don't fix it.
