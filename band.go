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
    "github.com/sourcefrog/conserve/conserve_proto"
    "os"
    "path"
)

const (
    // TODO: Generate names numerically so we can store more than one band.
    firstBandNumber  = "0000"
    BandHeadFilename = "BANDHEAD"
    BandTailFilename = "BANDTAIL"
)

type BandWriter struct {
    archive   *Archive
    name      string
    directory string
}

func CreateBand(archive *Archive) (band *BandWriter, err error) {
    name := firstBandNumber
    band = &BandWriter{
        archive:   archive,
        name:      name,
        directory: path.Join(archive.Directory(), name),
    }
    err = os.Mkdir(band.directory, 0777)
    if err != nil {
        return
    }
    header := &conserve_proto.BandHead{}
    header.BandNumber = &name
    stamp := MakeStamp()
    header.Stamp = &stamp
    err = WriteProtoToFile(header,
        path.Join(band.directory, BandHeadFilename))
    return
}

func (b *BandWriter) Name() string {
    return b.name
}

func (b *BandWriter) Directory() string {
    return b.directory
}
