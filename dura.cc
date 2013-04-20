#include <iostream> 

#include <boost/program_options.hpp>

#include "proto/dura.pb.h"

#include "archive.h"

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

namespace dura {

namespace po = boost::program_options;


int parse_options(int argc, char *argv[]) {
    string command;
    string archive_base_dir;

    po::options_description desc("Allowed options");
    desc.add_options()
        ("help", "show help message")
        ("command", po::value<string>(&command), "command to run")
        ("archive-dir", po::value(&archive_base_dir));

    po::positional_options_description posopts;
    posopts.add("command", 1);

    po::variables_map vm;
    po::command_line_parser parser(argc, argv);
    parser.options(desc);
    parser.positional(posopts);

    po::store(parser.run(), vm);
    po::notify(vm);

    if (vm.count("help")) {
        cout << desc << "\n";
        return 1;
    }
    if (!command.length()) {
        cout << "no command given!\n";
        return 1;
    } else if (command == "init-archive") {
        if (archive_base_dir.empty()) {
            cout << "no archive-dir specified\n";
            return 1;
        }
        Archive* arch = Archive::create(archive_base_dir);
    } else {
        cout << "command: " << command << "\n";
        return 0;
    }

    return 0;
}

} // namespace dura


int main(int argc, char *argv[]) {
    if (dura::parse_options(argc, argv))
        return 1;
    return 0;
}
