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

#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>

#include <boost/filesystem.hpp>

#include <glog/logging.h>

#include <google/protobuf/text_format.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

#include "proto/conserve.pb.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;
using namespace google::protobuf::io;
using namespace google::protobuf;
using namespace conserve::proto;

void write_proto_to_file(
        const Message& message,
	const filesystem::path& path) {
    int fd = open(path.string().c_str(),
	    O_CREAT|O_EXCL|O_WRONLY,
	    0666);
    PCHECK(fd > 0);
    CHECK(message.SerializeToFileDescriptor(fd));
    int ret = close(fd);
    PCHECK(ret == 0);
}


void read_proto_from_file(
        const boost::filesystem::path path,
        Message* message) {
    int fd = open(path.c_str(), O_RDONLY);
    PCHECK(fd > 0);
    CHECK(message->ParseFromFileDescriptor(fd));
    int ret = close(fd);
    PCHECK(ret == 0);
}


string gethostname_str() {
    char hostname[256];
    gethostname(hostname, sizeof hostname - 1);
    return string(hostname);
}

void populate_stamp(Stamp *stamp) {
    stamp->set_unixtime(time(0));
    stamp->set_hostname(gethostname_str());
    stamp->set_software_version(PACKAGE_VERSION);
}

} // namespace conserve

// vim: sw=4 et
