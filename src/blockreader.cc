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
#include "block.h"
#include "blockreader.h"
#include "filecopy.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;
using namespace boost::filesystem;


BlockReader::BlockReader(path directory, int block_number) :
    Block(directory, block_number),
    data_reader_(data_filename_),
    file_number_(0)
{
    read_proto_from_file(index_path_, &index_pb_, "block", "index");
}


path BlockReader::file_path() const {
    CHECK(file_number_ < index_pb_.file_size());
    return unpack_path(index_pb_.file(file_number_).path());
}


const proto::FileIndex& BlockReader::file_index() const {
    return index_pb_.file(file_number_);
}


bool BlockReader::done() const {
    return file_number_ >= index_pb_.file_size();
}


void BlockReader::advance() {
    CHECK(!done());
    file_number_++;
}


void BlockReader::restore_file(const path &restore_path) {
    // TODO: Restore file mode; even if it's looser than the current umask.
    int to_fd = open(restore_path.c_str(),
            O_CREAT | O_EXCL | O_NOFOLLOW | O_WRONLY,
            0666);
    PCHECK(to_fd != -1);
    const proto::FileIndex& file_index = index_pb_.file(file_number_);
    LOG(INFO) << "attempt to restore " << file_index.data_length()
        << " bytes to " << restore_path.string();
    data_reader_.extract_to_fd(file_index.data_length(), to_fd);
    PCHECK(!close(to_fd));
}


} // namespace conserve

// vim: sw=4 et
