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
)

const (
    // TODO: Generate names numerically so we can store more than one band.
    firstBandName = "0000"
    BandHeadFilename = "BANDHEAD"
    BandTailFilename = "BANDTAIL"
)

type BandWriter struct {
    archive *Archive
    name string
}

func CreateBand(a *Archive) (band *BandWriter, err error) {
    // TODO: Write header
    name := firstBandName
    return &BandWriter{archive: a, name: name}, nil
}

func (b *BandWriter) Name() string {
    return b.name
}
