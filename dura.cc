#include <iostream>

#include <boost/program_options.hpp>

#include <gflags/gflags.h>
#include <glog/logging.h>

#include "proto/dura.pb.h"

#include "archive.h"

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

DEFINE_string(
    archive_dir,
    "",
    "Path of backup archive.");

namespace dura {


int parse_options(int argc, char *argv[]) {
    if (argc < 2) {
        cout << "no command given!\n";
        return 1;
    }
    string command(argv[1]);
    if (command == "init-archive") {
        if (FLAGS_archive_dir.empty()) {
            cout << "no archive-dir specified\n";
            return 1;
        }
        Archive::create(FLAGS_archive_dir);
    } else {
        cout << "command: " << command << "\n";
        return 0;
    }

    return 0;
}

} // namespace dura


int main(int argc, char *argv[]) {
    google::InitGoogleLogging(argv[0]);
    google::ParseCommandLineFlags(&argc, &argv, true);
    if (dura::parse_options(argc, argv))
        return 1;
    return 0;
}
