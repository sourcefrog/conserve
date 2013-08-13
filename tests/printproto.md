You can use the 'printproto' command to print the contents of an archive
control file in human-readable form:

    $ conserve init-archive a
    $ conserve printproto a/CONSERVE
    magic: "conserve archive"

printproto takes exactly one argument:

    $ conserve -L printproto
    'conserve printproto' takes one argument, the path of the file to dump.
    [1]
    $ conserve -L printproto 1 2 3
    'conserve printproto' takes one argument, the path of the file to dump.
    [1]

protobuf messages don't carry any overall type identification, so printproto
infers the format from the last component of the filename.  It complains if it
can't guess the format:

    $ conserve -L printproto /dev/null
    can't infer proto format from filename "/dev/null"
    [1]

You can also print band heads and tails, and block indexes:

    $ echo hello world > hello
    $ conserve backup hello a
    $ conserve printproto a/b0000/BANDHEAD
    band_number: "0000"
    stamp {
      unixtime: \d+ (re)
      hostname: "*" (glob)
      software_version: "0.1.0"
    }
    $ conserve printproto a/b0000/BANDTAIL
    band_number: "0000"
    stamp {
      unixtime: \d+ (re)
      hostname: "*" (glob)
      software_version: "0.1.0"
    }
    $ conserve printproto a/b0000/a000000
    file {
      path: "hello"
      data_length: 12
    }
    stamp {
      unixtime: \d+ (re)
      hostname: "*" (glob)
      software_version: "0.1.0"
    }
    $ conserve -L printproto a/b0000/d000000
    "a/b0000/d000000" is a block data file and they don't contain protos
    [1]

TODO(mbp): Check block count in tail

