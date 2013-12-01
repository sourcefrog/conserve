package conserve

import (
    "os"
)

type Archive struct {
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
    // TODO(mbp): Actually write the header
    archive = &Archive{}
    return
}
