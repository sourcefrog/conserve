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

    "github.com/sourcefrog/conserve/conserve_proto"
)

func createTestDirectory() (string, error) {
    return ioutil.TempDir("", "conserve_test_")
}

func createTestArchive(t *testing.T) (archive *Archive, err error) {
    testDir, err := createTestDirectory()
    if err != nil {
        t.Error(err.Error())
    }
    archive, err = InitArchive(testDir)
    if err != nil {
        t.Error(err.Error())
    }
    if archive == nil {
        t.Error("nil archive returned")
    }
    return
}

func TestInitArchive(t *testing.T) {
    archive, _ := createTestArchive(t)

    archiveHeadFilename := archive.Directory() + "/CONSERVE"
    f, err := os.Open(archiveHeadFilename)
    if err != nil {
        t.Error("failed to read archive magic: ", err)
        return
    }

    magic := make([]byte, 100)
    n, err := f.Read(magic)
    if err != nil {
        t.Error("failed to read archive magic: ", err)
        return
    }

    var expected_magic = ("\x0a\x17conserve backup archive")
    var got_magic = string(magic[:n])
    if got_magic != expected_magic {
        t.Errorf("wrong archive magic: wanted %q got %q",
            expected_magic, got_magic)
    }

    var headPb = &conserve_proto.ArchiveHead{}
    err = ReadProtoFromFile(headPb, archiveHeadFilename)
    if err != nil {
        t.Errorf("failed to parse head proto: %v", err)
    }
    if *headPb.Magic != ArchiveMagicString {
        t.Errorf("bad decoded archive magic: %#v", *headPb.Magic)
    }
}

func TestOpenArchive(t *testing.T) {
    archive, err := createTestArchive(t)
    testDir := archive.Directory()
    archive2, err := OpenArchive(testDir)
    if archive2 == nil || err != nil {
        t.Errorf("failed to open archive %v: %v",
            testDir, err)
    }
}

func TestOpenNoHeader(t *testing.T) {
    testDir, err := createTestDirectory()
    archive2, err := OpenArchive(testDir)
    if archive2 != nil || err == nil {
        t.Errorf("expected failure, was disappointed")
    }
}
