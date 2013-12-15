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
    "os"

    "code.google.com/p/goprotobuf/proto"
)


func writeProtoToFile(message proto.Message, filename string) (err error) {
    bytes, err := proto.Marshal(message)
    if err != nil {
        return
    }

    f, err := os.Create(filename)
    if err != nil {
        return
    }

    _, err = f.Write(bytes)
    if err != nil {
        f.Close()
        return
    }

    err = f.Close()
    return
}
