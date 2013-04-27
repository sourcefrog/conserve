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

#include <sys/types.h>
#include <sys/stat.h>
#include <unistd.h>
#include <fcntl.h>

#include <boost/filesystem.hpp>

#include <glog/logging.h>

#include <google/protobuf/text_format.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

#include "proto/conserve.pb.h"

#include "archive.h"

namespace conserve {

using namespace std;

using namespace boost;

using namespace google::protobuf::io;
using namespace google::protobuf;

void write_proto_to_file(const Message& message,
	const filesystem::path& path) {
    int fd = open(path.string().c_str(),
	    O_CREAT|O_EXCL|O_WRONLY,
	    0666);
    PCHECK(fd > 0);
    CHECK(message.SerializeToFileDescriptor(fd));
    int ret = close(fd);
    PCHECK(ret == 0);
}


void write_archive_header(const filesystem::path& base_dir) {
    LOG(INFO) << "create archive in " << base_dir;
    conserve::proto::ArchiveHeader header;
    header.set_magic("conserve archive");
    write_proto_to_file(header, base_dir/"CONSERVE-ARCHIVE");
}

Archive* Archive::create(const string dir) {
    filesystem::path base_path(dir);
    filesystem::create_directory(base_path);
    write_archive_header(base_path);

    return new Archive(dir);
}

} // namespace conserve
