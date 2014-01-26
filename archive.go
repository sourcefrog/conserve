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
    "log"
    "os"

    "code.google.com/p/goprotobuf/proto"
    "github.com/sourcefrog/conserve/conserve_proto"
)

const (
    ArchiveMagicFile   string = "CONSERVE"
    ArchiveMagicString        = "conserve backup archive"
)

type Archive struct {
    dir string
}

func (archive Archive) Directory() string {
    return archive.dir
}

func InitArchive(archive_dir string) (archive *Archive, err error) {
    err = os.Mkdir(archive_dir, 0777)
    if os.IsExist(err) {
        // Already exists; no problem
        err = nil
        // TODO(mbp): Check an existing directory is empty.
    } else if err != nil {
        return
    }

    err = writeArchiveHeader(archive_dir)
    if err != nil {
        return
    }

    archive = &Archive{dir: archive_dir}
    return
}

func headName(archive_dir string) string {
    return archive_dir + "/" + ArchiveMagicFile
}

func writeArchiveHeader(archive_dir string) (err error) {
    header := &conserve_proto.ArchiveHead{
        Magic: proto.String(ArchiveMagicString),
        // TODO: set stamp
    }
    err = WriteProtoToFile(header, headName(archive_dir))
    return
}

func OpenArchive(archive_dir string) (archive *Archive, err error) {
    head_name := headName(archive_dir)
    head_file, err := os.Open(head_name)
    if head_file == nil {
        log.Printf("no header file found in %v, %v", archive_dir, err)
        return
    }
    defer head_file.Close()

    // TODO: check magic

    return &Archive{dir: archive_dir}, nil
}
