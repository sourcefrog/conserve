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

package conserve_test

import (
    "testing"
    "github.com/sourcefrog/conserve"
)

func TestCreateBand(t *testing.T) {
    archive, err := createTestArchive(t)
    band, err := conserve.CreateBand(archive)
    if band == nil || err != nil {
        t.Errorf("failed to create band: %v", err)
        return
    }
    if band.Name() != "0000" {
        t.Errorf("unexpected band name %#v", band.Name())
    }
}
