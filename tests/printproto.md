You can use the 'printproto' command to print the contents of an archive
control file in human-readable form:

    $ conserve init-archive a
    $ conserve printproto a/CONSERVE-ARCHIVE
    magic: "conserve archive"

printproto takes exactly one argument:

    $ conserve printproto 
    E*] 'conserve printproto' takes exactly one argument, the path of the file to dump. (glob)
    [1]
    $ conserve printproto 1 2 3
    E*] 'conserve printproto' takes exactly one argument, the path of the file to dump. (glob)
    [1]

protobuf messages don't carry any overall type identification, so printproto
infers the format from the last component of the filename.  It complains if it
can't guess the format:

    $ conserve printproto /dev/null
    E*] can't infer proto format from filename "/dev/null" (glob)
    [1]

You can also print band heads and tails, and block indexes:

    $ echo hello > srcfile
    $ conserve backup a hello
    $ conserve printproto a/b0000/BAND-HEAD
    band_number: "0000"
    start_unixtime: \d+ (re)
    source_hostname: "*" (glob)
    $ conserve printproto a/b0000/BAND-TAIL
    band_number: "0000"
    end_unixtime: \d+ (re)

TODO(mbp): Check block count in tail

TODO(mbp): Check block index

