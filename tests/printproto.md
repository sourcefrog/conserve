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
