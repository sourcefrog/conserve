// Conserve - robust backup system
// Copyright 2012-2013 Martin Pool
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

#include <getopt.h>
#include <iostream>
#include <unistd.h>

#include <boost/program_options.hpp>

#include <glog/logging.h>

#include "proto/conserve.pb.h"

#include "archive.h"
#include "band.h"

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

namespace conserve {

const string version = "0.0";

const string usage =
"conserve - A robust backup program\n"
"\n"
"Copyright 2012-2013 Martin Pool\n"
"Licenced under the Apache Licence, Version 2.0.\n"
"\n"
"Options:\n"
"  -h            Show help.\n"
"  -V            Show version.\n";


void show_help() {
    cout << usage;
}

} // namespace conserve

using namespace conserve;


int main(int argc, char *argv[]) {
    google::InitGoogleLogging(argv[0]);
    google::SetStderrLogging(google::GLOG_INFO);

    int opt;
    while (true) {
        opt = getopt(argc, argv, "hV");
        if (opt == 'h') {
            show_help();
            return 0;
        } else if (opt == 'V') {
            cout << "conserve " << version << "\n";
            return 0;
        } else if (opt == -1)
            break;
        else {
            LOG(FATAL) << "Unexpected getopt result " << (char) opt;
        }
    }

    if (!argv[optind]) {
        LOG(ERROR) << "No command given";
        return 1;
    }
    string command(argv[optind]);
    if (command == "init-archive") {
        const char *archive_dir = argv[optind+1];
        if (!archive_dir) {
            LOG(ERROR) << "Usage: init-archive ARCHIVE-DIR";
            return 1;
        }
        Archive::create(archive_dir);
    } else if (command == "backup") {
        const char *archive_dir = argv[optind+1];
        if (!archive_dir) {
            LOG(ERROR) << "Usage: init-archive ARCHIVE-DIR";
            return 1;
        }
        // TODO(mbp): Change to a given directory to read the source 
        // files, so that their relative paths are correct.  Perhaps also,
        // an option to strip a given prefix off the names.
        Archive archive(archive_dir);
        BandWriter band = archive.start_band();
        band.finish();
    } else {
        LOG(ERROR) << "Unrecognized command: " << command;
        return 0;
    }

    return 0;
}

// vim: sw=4 et
