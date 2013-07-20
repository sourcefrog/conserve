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

#include <glog/logging.h>

#include "archive.h"
#include "band.h"
#include "block.h"

namespace conserve {

int do_backup(char **argv) {
    const char *archive_dir = argv[0];
    if (!archive_dir) {
            LOG(ERROR) << "Usage: conserve backup ARCHIVE SOURCE...";
            return 1;
    }
    // TODO(mbp): Change to a given directory to read the source 
    // files, so that their relative paths are correct.  Perhaps also,
    // an option to strip a given prefix off the names.
    Archive archive(archive_dir);
    BandWriter band = archive.start_band();
    BlockWriter block(band);
    block.start();
    // TODO(mbp): Actually back up the files!
    block.finish();
    band.finish();

    return 0;
}
} // namespace conserve

// vim: sw=4 et
