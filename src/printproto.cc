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

#include <boost/filesystem.hpp>

#include <glog/logging.h>

#include <google/protobuf/io/zero_copy_stream_impl.h>
#include <google/protobuf/text_format.h>

#include "proto/conserve.pb.h"

#include "archive.h"
#include "band.h"
#include "block.h"
#include "printproto.h"
#include "util.h"


using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;

namespace conserve {

int cmd_printproto(char **args) {
    if (!args[0] || args[1]) {
        LOG(ERROR) << "'conserve printproto' takes exactly one argument, "
            << "the path of the file to dump.";
        return 1;
    }

    const boost::filesystem::path path = args[0];
    google::protobuf::Message* message;
    const boost::filesystem::path filename = path.filename();

    if (filename == Archive::HEAD_NAME) {
        message = new conserve::proto::ArchiveHead();
    } else if (filename == Band::HEAD_NAME) {
        message = new conserve::proto::BandHead();
    } else if (filename == Band::TAIL_NAME) {
        message = new conserve::proto::BandTail();
    } else if (Block::resembles_index_filename(filename.string())) {
        message = new conserve::proto::BlockIndex();
    } else {
    	LOG(ERROR) << "can't infer proto format from filename " << path;
    	return 1;
    }

    // TODO(mbp): Handle files that are compressed, encrypted, etc.
    read_proto_from_file(path, message);
    google::protobuf::io::FileOutputStream outstream(1);
    TextFormat::Print(*message, &outstream);
    outstream.Flush();

    delete message;

    return 0;
}

} // namespace conserve

// vim: sw=4 et
