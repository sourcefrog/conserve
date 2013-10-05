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
#include "blockreader.h"
#include "blockwriter.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;


const string Band::HEAD_NAME = "BANDHEAD";
const string Band::TAIL_NAME = "BANDTAIL";


Band::Band(Archive* archive, string name) :
    archive_(archive),
    name_(name),
    band_directory_(archive->base_dir_ / ("b" + name))
{
}


BandWriter::BandWriter(Archive *archive, string name) :
    Band(archive, name),
    next_block_number_(0)
{
}


path Band::head_file_name() const {
    return band_directory_ / Band::HEAD_NAME;
}


path Band::tail_file_name() const {
    return band_directory_ / Band::TAIL_NAME;
}


void BandWriter::start() {
    LOG(INFO) << "start band in " << band_directory_;
    filesystem::create_directory(band_directory_);
    proto::BandHead head_pb;
    head_pb.set_band_number(name_);
    populate_stamp(head_pb.mutable_stamp());
    write_proto_to_file(head_pb,
        head_file_name());
}


BlockWriter BandWriter::start_block() {
    return BlockWriter(directory(), 0);
}


void BandWriter::finish() {
    proto::BandTail tail_pb;
    tail_pb.set_band_number(name_);
    populate_stamp(tail_pb.mutable_stamp());
    // TODO(mbp): Write block count
    write_proto_to_file(tail_pb, tail_file_name());
    LOG(INFO) << "finish band in " << band_directory_;
}


int BandWriter::next_block_number() {
    // TODO(mbp): Needs to be improved if the band's partially complete.
    return next_block_number_++;
}


BandReader::BandReader(Archive *archive, string name) :
    Band(archive, name),
    current_block_number_(-1)
{
    read_proto_from_file(head_file_name(), &head_pb_, "band", "head");
    read_proto_from_file(tail_file_name(), &tail_pb_, "band", "tail");
    LOG(INFO) << "start reading band " << head_pb_.band_number();
    CHECK(head_pb_.band_number() == tail_pb_.band_number());
    CHECK(tail_pb_.block_count() >= 0);
}


bool BandReader::done() const {
    return current_block_number_ >= tail_pb_.block_count();
}


BlockReader BandReader::read_next_block() {
    current_block_number_++;
    return BlockReader(directory(), current_block_number_);
}


} // namespace conserve

// vim: sw=4 et
