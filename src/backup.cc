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
#include "blockwriter.h"
#include "exitcode.h"

namespace conserve {

using namespace boost::filesystem;

enum ExitCode cmd_backup(char **argv) {
    vector<path> source_names;
    path archive_dir;

    for (int i = 0; argv[i]; i++)
        if (argv[i+1])
            source_names.push_back(path(argv[i]));
        else
            archive_dir = path(argv[i]);

    if (source_names.empty()) {
        LOG(ERROR) << "Usage: conserve backup SOURCE... ARCHIVE";
        return EXIT_COMMAND_LINE;
    }

    // TODO(mbp): Change to a given directory to read the source
    // files, so that their relative paths are correct.  Perhaps also,
    // an option to strip a given prefix off the names.
    // TODO(mbp): Normalize path, check it doesn't contain ..

    Archive archive(archive_dir, false);

    BandWriter band = archive.start_band();
    BlockWriter block = band.start_block();

    // TODO: Make sure to add the files in the right order.
    for (unsigned i = 0; i < source_names.size(); i++) {
        block.add_file(source_names[i]);
    }

    block.finish();
    band.finish();

    return EXIT_OK;
}
} // namespace conserve

// vim: sw=4 et
