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
#include <ctype.h>

#include <boost/filesystem.hpp>
#include <boost/format.hpp>

#include <glog/logging.h>

#include "proto/conserve.pb.h"

#include "archive.h"
#include "band.h"
#include "blockwriter.h"
#include "filecopy.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;
using namespace boost::filesystem;

const size_t copy_buf_size = 64 << 10;


BlockWriter::BlockWriter(path directory, int block_number) :
    Block(directory, block_number),
    data_writer_(data_filename_)
{
}


void BlockWriter::add_file(const path& source_path) {
    CHECK(source_path > last_path_stored_)
        << source_path.string() << ", " << last_path_stored_.string();

    int64_t content_len = -1;
    data_writer_.store_file(source_path, &content_len);
    CHECK(content_len >= 0);

    proto::FileIndex* file_index = index_proto_.add_file();
    break_path(source_path, file_index->mutable_path());
    file_index->set_data_length(content_len);

    last_path_stored_ = source_path;
}


void BlockWriter::finish() {
    // TODO: Finish the data block first to check it's complete?

    populate_stamp(index_proto_.mutable_stamp());

    // TODO: Accumulate size and hash as we write the data file, and store it
    // into the index.
    index_proto_.set_compression(proto::BZIP2);
    write_proto_to_file(index_proto_, index_filename_);
    LOG(INFO) << "write block index in " << index_filename_;
}


} // namespace conserve

// vim: sw=4 et
