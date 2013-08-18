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

#include <getopt.h>
#include <iostream>
#include <unistd.h>

#include <boost/program_options.hpp>

#include <glog/logging.h>

#include "proto/conserve.pb.h"

#include "archive.h"
#include "backup.h"
#include "printproto.h"

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

namespace conserve {

const string version = PACKAGE_VERSION;

const string usage =
"conserve - a robust backup program\n"
"\n"
"Copyright 2012-2013 Martin Pool\n"
"Licenced under the GNU General Public Licence, version 2 or later.\n"
"Conserve comes with ABSOLUTELY NO WARRANTY of any kind.\n"
"\n"
"Options:\n"
"  -h            Show help.\n"
"  -v            Show info logs on stderr.\n"
"  -V            Show version.\n"
"  -L            Suppress severity/date/time/source prefix on log lines.\n"
"\n"
"Commands:\n"
"  backup SOURCE... ARCHIVE\n"
"  init-archive DIR\n"
"  printproto FILE\n"
;


void show_help() {
    cout << usage;
}

} // namespace conserve

using namespace conserve;


int main(int argc, char *argv[]) {
    // TODO(mbp): Log files might contain sensitive information. They are
    // written out with permissions controlled by the umask by default.
    // Possibly we should make them tighter. Or perhaps people should just set
    // their umask?
    google::InitGoogleLogging(argv[0]);
    google::SetStderrLogging(google::WARNING);

    int opt;
    while (true) {
        opt = getopt(argc, argv, "hLVv");
        if (opt == 'h') {
            show_help();
            return 0;
        } else if (opt == 'V') {
            cout << "conserve " << version << "\n";
            return 0;
        } else if (opt == 'v') {
            google::SetStderrLogging(google::INFO);
        } else if (opt == 'L') {
            FLAGS_log_prefix = false;
        } else if (opt == -1)
            break;
        else {
            LOG(FATAL) << "Unexpected getopt result " << (char) opt;
        }
    }

    if (!argv[optind]) {
        LOG(ERROR) << "please give a command or use 'conserve -h' for help";
        return 1;
    }
    string command(argv[optind]);
    char **command_args = &argv[optind+1];
    if (command == "init-archive") {
        const char *archive_dir = command_args[0];
        if (!archive_dir) {
            LOG(ERROR) << "Usage: init-archive ARCHIVE-DIR";
            return 1;
        }
        Archive::create(archive_dir);
    } else if (command == "backup") {
        return do_backup(command_args);
    } else if (command == "printproto") {
        return cmd_printproto(command_args);
    } else {
        LOG(ERROR) << "Unrecognized command: " << command;
        return 0;
    }

    return 0;
}

// vim: sw=4 et
