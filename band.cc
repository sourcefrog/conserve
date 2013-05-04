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
#include "band.h"

namespace conserve {

using namespace std;
using namespace boost;


BandWriter::BandWriter(Archive* archive, string name) : 
    archive_(archive), 
    name_(name), 
    band_directory_(archive->base_dir_ / ("b" + name))
{
}

void BandWriter::start() {
    LOG(INFO) << "start band in " << band_directory_;
    filesystem::create_directory(band_directory_);
    // TODO(mbp): Write band head
}

void BandWriter::finish() {
    // TODO(mbp): Write band tail
}

} // namespace conserve

// vim: sw=4 et
