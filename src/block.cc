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
#include <boost/format.hpp>

#include <glog/logging.h>

#include "proto/conserve.pb.h"

#include "archive.h"
#include "band.h"
#include "block.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;
using namespace boost::filesystem;


BlockWriter::BlockWriter(BandWriter band_writer) :
    block_directory_(band_writer.directory()),
    block_number_(band_writer.next_block_number())
{
    string padded_number = (boost::format("%06d") % block_number_).str();
    index_filename_ = block_directory_ / ("a" + padded_number);
    data_filename_ = block_directory_ / ("d" + padded_number);
}

void BlockWriter::start() {
    data_fd_ = open(data_filename_.string().c_str(),
        O_CREAT|O_EXCL|O_WRONLY,
        0666);
    PCHECK(data_fd_ > 0);
}

void BlockWriter::finish() {
    int ret = close(data_fd_);
    PCHECK(ret == 0);

    populate_stamp(index_proto_.mutable_stamp());

    // TODO(mbp): Compress it.
    write_proto_to_file(index_proto_, index_filename_);
    LOG(INFO) << "write block index in " << index_filename_;
}

} // namespace conserve

// vim: sw=4 et
