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

#include <sys/stat.h>
#include <sys/types.h>

#include <string>
#include <vector>

#include <glog/logging.h>

#include "archive.h"
#include "band.h"
#include "block.h"
#include "blockreader.h"

namespace conserve {

using namespace boost::filesystem;

int cmd_restore(char **argv) {
    // TODO: Restore selected files or directories.
    // TODO: Choose which band, based on name or date.

    if (!argv[0] || !argv[1] || argv[2]) {
        LOG(ERROR) << "usage: conserve restore ARCHIVE TODIR";
        return 1;
    }
    const path archive_dir = argv[0];
    const path restore_dir = argv[1];

    if (mkdir(restore_dir.c_str(), 0777)) {
        if (errno == EEXIST) {
            LOG(ERROR)
                << "error creating restore destination directory \""
                << restore_dir.string()
                << "\": " << strerror(errno);
        }
        return 1;
    }

    Archive archive(archive_dir, false);
    BandReader band(&archive, archive.last_band_name());

    // TODO: Change to more idiomatic C++ iterators?
    // TODO: Read all bands.
    while (!band.done()) {
        for (BlockReader block_reader = band.read_next_block();
             !block_reader.done();
             block_reader.advance()) {
            const proto::FileIndex &file_index(
                block_reader.file_index());
            const path file_path(block_reader.file_path());
            LOG(INFO) << "restore file #" << block_reader.file_number()
                << " path="
                << file_path.string();
            CHECK(file_index.file_type() == proto::REGULAR);
            block_reader.restore_file(restore_dir / file_path);
        }
    }

    return 0;
}

} // namespace conserve

// vim: sw=4 et
