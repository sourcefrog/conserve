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
#include "validate.h"

namespace conserve {

using namespace boost::filesystem;

int cmd_validate(char **argv) {
    if (!argv[0] || argv[1]) {
        LOG(ERROR) << "usage: conserve validate ARCHIVE";
        return 1;
    }
    const path archive_dir = argv[0];

    Archive archive(archive_dir, false);
    BandReader band(&archive, archive.last_band_name());

    // TODO: Read all bands.
    while (!band.done()) {
        // TODO: Check number of blocks is as expected.
        for (BlockReader block_reader = band.read_next_block();
             !block_reader.done();
             block_reader.advance()) {
            const proto::FileIndex &file_index(
                block_reader.file_index());
            const path file_path(block_reader.file_path());
            LOG(INFO) << "Validate file #" << block_reader.file_number()
                << " path=" << file_path.string();
            CHECK(file_index.file_type() == proto::REGULAR);
            // TODO: Decompress file, check hash and length.
            // TODO: Check file name ordering.
        }
    }

    return 0;
}

} // namespace conserve

// vim: sw=4 et
