// Conserve - robust backup system
// Copyright 2014 Martin Pool
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
    "time"
)

func MakeStamp() (stamp conserve_proto.Stamp) {
    stamp.UnixTime = new(int64)
    *stamp.UnixTime = time.Now().Unix()
    hostname, _ := os.Hostname()
    stamp.Hostname = &hostname
    version := ConserveVersion
    stamp.SoftwareVersion = &version
    return
}
