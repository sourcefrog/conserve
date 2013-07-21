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
#include <time.h>

#include <boost/filesystem.hpp>

#include <glog/logging.h>

#include <google/protobuf/text_format.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

#include "proto/conserve.pb.h"

#include "archive.h"
#include "band.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;


const string Band::HEAD_NAME = "BAND-HEAD";
const string Band::TAIL_NAME = "BAND-TAIL";


BandWriter::BandWriter(Archive* archive, string name) : 
    archive_(archive), 
    name_(name), 
    band_directory_(archive->base_dir_ / ("b" + name)),
    next_block_number_(0)
{
}

void BandWriter::start() {
    LOG(INFO) << "start band in " << band_directory_;
    filesystem::create_directory(band_directory_);
    proto::BandHead head_pb;
    head_pb.set_band_number(name_);
    head_pb.set_start_unixtime(time(NULL));
    head_pb.set_source_hostname(gethostname_str());
    write_proto_to_file(head_pb,
            band_directory_ / Band::HEAD_NAME);
}

void BandWriter::finish() {
    proto::BandTail tail_pb;
    tail_pb.set_band_number(name_);
    tail_pb.set_end_unixtime(time(NULL));
    write_proto_to_file(tail_pb,
            band_directory_ / Band::TAIL_NAME);
    LOG(INFO) << "finish band in " << band_directory_;
}

int BandWriter::next_block_number() {
    // TODO(mbp): Needs to be improved if the band's partially complete.
    return next_block_number_++;
}

} // namespace conserve

// vim: sw=4 et
