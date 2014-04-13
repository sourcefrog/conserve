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

type BlockWriter struct {
    blockIndex  conserve_proto.BlockIndex
    dataFile    *os.File
    directory   string
    blockNumber string
    finished    bool
}

func StartBlock(bandw *BandWriter) (blkw *BlockWriter, err error) {
    // TODO: Increment numbers
    AssertNotFinished(bandw.Finished())
    blockNumber := "0000000"
    blockBaseName := path.Join(bandw.Directory(), "d"+blockNumber)
    dataFile, err := os.OpenFile(
        blockBaseName,
        os.O_WRONLY|os.O_CREATE|os.O_EXCL,
        0666)
    if err != nil {
        return
    }
    blkw = &BlockWriter{
        dataFile:    dataFile,
        directory:   bandw.Directory(),
        blockNumber: blockNumber,
        blockIndex: conserve_proto.BlockIndex{
            File: make([]*conserve_proto.FileIndex, 0, 0),
        },
    }
    return
}

func (blkw *BlockWriter) AddFile(sourceFile *os.File) (err error) {
    // Add to index
    // TODO: Trim off some of the name depending on the base directory.
    AssertNotFinished(blkw.finished)

    fileType := conserve_proto.FileType_REGULAR
    newFileIndex := conserve_proto.FileIndex{
        FileType: &fileType,
        Path:     []byte(sourceFile.Name()),
    }
    blkw.blockIndex.File = append(blkw.blockIndex.File, &newFileIndex)

    // TODO: Write compressed
    sourceFile.Seek(0, os.SEEK_SET)
    // TODO: Copy everything.
    // TODO: Accumulate hash as we go.
    buf := make([]byte, 60000)
    bytesRead, err := sourceFile.Read(buf)
    if err != nil {
        return
    }
    _, err = blkw.dataFile.Write(buf[:bytesRead])
    if err != nil {
        return
    }

    return
}

func (blkw *BlockWriter) Finish() (err error) {
    blkw.finished = true
    indexFileName := path.Join(blkw.directory, "a"+blkw.blockNumber)
    blkw.blockIndex.Stamp = MakeStamp()
    err = WriteProtoToFile(
        &blkw.blockIndex,
        indexFileName)
    return
}
