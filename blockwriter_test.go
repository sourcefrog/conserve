// Conserve - robust backup system
// Copyright 2012-2013 Martin Pool
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
    "io/ioutil"
    "os"
    "testing"
)

func TestAddFiles(t *testing.T) {
    archive, err := createTestArchive(t)
    band, err := StartBand(archive)
    if band == nil || err != nil {
        t.Errorf("failed to create band: %v", err)
        return
    }

    tempfile, err := ioutil.TempFile("", "testsource")
    defer os.Remove(tempfile.Name())
    tempfile.Write([]byte("hello world!\n"))
    defer tempfile.Close()

    blkw, err := StartBlock(band)
    // TODO: Strip off base-directory path.
    blkw.AddFile(tempfile)

    err = blkw.Finish()
    if err != nil {
        t.Fail()
    }

    // TODO: Test reading content back.
}
