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

package conserve

import (
    "os"
)

func Backup(archive *Archive, names []string) (err error) {
    bandw, err := StartBand(archive)
    if err != nil {
        return
    }
    blockw, err := StartBlock(bandw)
    if err != nil {
        return
    }
    for _, filename := range names {
        file, err := os.Open(filename)
        if err != nil {
            return err
        }
        defer file.Close()
        err = blockw.AddFile(file)
        if err != nil {
            return err
        }
    }
    err = blockw.Finish()
    if err != nil {
        return
    }
    err = bandw.Finish()
    if err != nil {
        return
    }
    return
}
