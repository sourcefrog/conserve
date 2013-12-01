package conserve

import (
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

func writeArchiveHeader(archive_dir string) (err error) {
    head_name := archive_dir + "/" + ArchiveMagicFile
    f, err := os.Create(head_name)
    if err != nil {
        return
    }

    header := &conserve_proto.ArchiveHead{
        Magic: proto.String(ArchiveMagicString),
        // TODO: set stamp
    }

    head_bytes, err := proto.Marshal(header)
    if err != nil {
        return
    }
    _, err = f.Write(head_bytes)
    if err != nil {
        return
    }

    err = f.Close()
    if err != nil {
        return
    }
    return
}
