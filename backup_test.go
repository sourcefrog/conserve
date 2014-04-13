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
    "io/ioutil"
    "testing"
)

func TestSimpleBackup(t *testing.T) {
    archive, err := createTestArchive(t)
    srcDir, err := createTestDirectory()
    srcFile, err := ioutil.TempFile(srcDir, "srcfile")
    srcFile.Write([]byte("hello"))
    err = Backup(archive, []string{srcFile.Name()})
    if err != nil {
        t.Error(err)
    }
}
