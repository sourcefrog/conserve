#include <getopt.h>
#include <iostream>
#include <unistd.h>

#include <boost/program_options.hpp>

#include <glog/logging.h>

#include "proto/conserve.pb.h"

#include "archive.h"

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

namespace conserve {

const string version = "0.0";

const string usage =
"conserve - A robust backup program\n"
"\n"
"Copyright 2012-2013 Martin Pool\n"
"Licenced under the Apache Licence, Version 2.0.\n";


int parse_options(int argc, char *argv[]) {
    if (argc < 2) {
        cout << "no command given!\n";
        return 1;
    }
    string command(argv[1]);
    if (command == "init-archive") {
        if (argc < 3) {
            cout << "no archive-dir specified\n";
            return 1;
        }
        Archive::create(argv[2]);
    } else {
        cout << "command: " << command << "\n";
        return 0;
    }

    return 0;
}

} // namespace conserve


int main(int argc, char *argv[]) {
    google::SetVersionString(conserve::version);
    google::SetUsageMessage(conserve::usage);
    google::InitGoogleLogging(argv[0]);
    if (conserve::parse_options(argc, argv))
        return 1;
    return 0;
}
