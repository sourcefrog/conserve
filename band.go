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
    archive    *Archive
    bandNumber string
    directory  string
    blockCount int32
}

func StartBand(archive *Archive) (band *BandWriter, err error) {
    bandNumber := firstBandNumber
    band = &BandWriter{
        archive:    archive,
        bandNumber: bandNumber,
        directory:  path.Join(archive.Directory(), bandNumber),
    }
    err = os.Mkdir(band.directory, 0777)
    if err != nil {
        return
    }
    header := &conserve_proto.BandHead{}
    header.BandNumber = &bandNumber
    header.Stamp = MakeStamp()
    err = WriteProtoToFile(header,
        path.Join(band.directory, BandHeadFilename))

    return
}

func (b *BandWriter) BandNumber() string {
    return b.bandNumber
}

func (b *BandWriter) Directory() string {
    return b.directory
}

func (b *BandWriter) Finish() (err error) {
    tail_pb := &conserve_proto.BandTail{
        BandNumber: &b.bandNumber,
        Stamp:      MakeStamp(),
        BlockCount: &b.blockCount,
    }
    err = WriteProtoToFile(tail_pb,
        path.Join(b.directory, BandTailFilename))

    return
}

// TODO: Open Band for read; scan through all blocks until done.

// TODO: Finish band and write footer.
