// Copyright 2013, Martin Pool

#include <assert.h>
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
    assert(fd > 0);
    assert(message.SerializeToFileDescriptor(fd));
    int ret = close(fd);
    assert(ret == 0);
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
