// Copyright 2013, Martin Pool

#include <google/protobuf/text_format.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

#include "proto/dura.pb.h"

#include "archive.h"

namespace dura {

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

Archive* Archive::create(const string dir) {
    duralib::proto::ArchiveHeader header;
    header.set_magic("dura archive");
    
    FileOutputStream out_stream(0);
    TextFormat::Print(header, &out_stream);

    return new Archive(dir);
}

} // namespace dura
