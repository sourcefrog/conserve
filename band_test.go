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
    "github.com/sourcefrog/conserve/conserve_proto"
    "os"
    "testing"
)

func TestCreateBand(t *testing.T) {
    archive, err := createTestArchive(t)
    band, err := CreateBand(archive)
    if band == nil || err != nil {
        t.Errorf("failed to create band: %v", err)
        return
    }
    if band.Name() != "0000" {
        t.Errorf("unexpected band name %#v", band.Name())
    }

    headName := band.Directory() + "/" + BandHeadFilename
    stat, err := os.Stat(headName)
    if stat == nil || err != nil {
        t.Errorf("failed to stat %v: %v", headName, err)
    }
    var head_pb conserve_proto.BandHead
    err = ReadProtoFromFile(&head_pb, headName)
    if err != nil {
        t.Errorf("failed to parse band head: %v", err)
    }
    if head_pb.BandNumber == nil || *head_pb.BandNumber != "0000" {
        t.Errorf("wrong number in band head: %v", head_pb.BandNumber)
    }
    // TODO: Check the stamp is plausible.
}
