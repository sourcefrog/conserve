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


#include <string>
#include <vector>

#include <glog/logging.h>

#include "archive.h"
#include "band.h"
#include "block.h"

namespace conserve {

using namespace boost::filesystem;

int cmd_restore(char **argv) {
    // TODO: Restore selected files or directories.
    // TODO: Choose which band, based on name or date.

    if (!argv[0] || !argv[1] || argv[2]) {
        LOG(ERROR) << "usage: conserve restore ARCHIVE DIR";
        return 1;
    }
    const path archive_dir = argv[0];
    const path restore_dir = argv[1];
    Archive archive(archive_dir);
    BandReader band(&archive, archive.last_band_name());

    // TODO: Change to more idiomatic C++ iterators?
    while (!band.done()) {
        BlockReader block_reader = band.read_next_block();
        while (!block_reader.done()) {
            LOG(INFO) << block_reader.file_number() << " "
                << block_reader.file_path().string();
            block_reader.advance();
        }
    }

    return 0;
}

} // namespace conserve

// vim: sw=4 et
