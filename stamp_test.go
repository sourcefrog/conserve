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
    "testing"
)

func TestMakeStamp(t *testing.T) {
    stamp := MakeStamp()

    CheckStamp(stamp, t)
}

func CheckStamp(stamp conserve_proto.Stamp, t *testing.T) {
    if *stamp.UnixTime == 0 {
        t.Errorf("unixtime not set")
    }
    if stamp.Hostname == nil {
        t.Errorf("hostname not set")
    }
    if stamp.SoftwareVersion == nil {
        t.Errorf("software_version not set")
    }
}
