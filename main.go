package main

import (
    "fmt"

    "github.com/sourcefrog/conserve/conservelib"
    "github.com/docopt/docopt.go" 
)

const usage = `
conserve - a robust backup program

Copyright 2012-2013 Martin Pool
Licenced under the GNU General Public Licence, version 2 or later.
Conserve comes with ABSOLUTELY NO WARRANTY of any kind.

Usage:
  conserve backup SOURCE... ARCHIVE
  conserve init DIR
  conserve printproto FILE
  conserve restore ARCHIVE DESTDIR
  conserve validate ARCHIVE

Options:
  -h            Show help.
  -v            Show info logs on stderr.
  -V            Show version.
  -L            Suppress severity/date/time/source prefix on log lines.
`


func main() {
    args, err := docopt.Parse(usage, nil, true,
	conservelib.ConserveVersion, false)
    fmt.Printf("args: %v\n", args)
    fmt.Printf("err: %v\n", err)
}
