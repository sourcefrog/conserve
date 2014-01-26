// Conserve - robust backup system
// Copyright 2012-2014 Martin Pool
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

package main

import (
    "flag"
    "fmt"
    "github.com/sourcefrog/conserve"
)

const usage = `conserve - a robust backup program

Copyright 2012-2013 Martin Pool
Licenced under the GNU General Public Licence, version 2 or later.
Conserve comes with ABSOLUTELY NO WARRANTY of any kind.

Usage:
  conserve [-v] init <dir>

Options:
  --help        Show help.
  --version     Show version.
  -v            Be more verbose.
`

// conserve backup <source>... <archive>
// conserve printproto <file>
// conserve restore <archive> <destdir>
// conserve validate <archive>

func main() {
    flag.Parse()
    cmd := flag.Arg(0)

    if flag.NArg() == 0 {
        fmt.Print(usage)
    } else if cmd == "init" {
        conserve.InitArchive(flag.Arg(1))
    }
}
