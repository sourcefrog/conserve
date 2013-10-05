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
#include "problem.h"
#include "util.h"

namespace conserve {

using namespace std;

using namespace boost;


const string Archive::_HARDCODED_SINGLE_BAND = "0000";
const string Archive::HEAD_NAME = "CONSERVE";
const string Archive::ARCHIVE_MAGIC = "conserve archive";


Archive::Archive(const path& base_dir, bool create) :
    base_dir_(base_dir)
{
    // TODO: Maybe separate class or function for creation rather than a bool?
    if (create) {
        LOG(INFO) << "create archive in " << base_dir_;
        filesystem::create_directory(base_dir_);
        write_archive_head();
    } else {
        LOG(INFO) << "open archive in " << base_dir_;
        path head_path = base_dir / HEAD_NAME;
        read_proto_from_file(head_path, &head_pb_, "archive", "head");
        if (head_pb_.magic() != ARCHIVE_MAGIC) {
            Problem("archive", "head", "bad-magic", head_path,
                    string("wrong magic: \"") + head_pb_.magic() + "\""
                    ).signal();
        }
    }
}


void Archive::write_archive_head() {
    LOG(INFO) << "create archive in " << base_dir_;
    head_pb_.set_magic(ARCHIVE_MAGIC);
    write_proto_to_file(head_pb_, base_dir_/HEAD_NAME);
}


string Archive::last_band_name() {
    return _HARDCODED_SINGLE_BAND;
}


BandWriter Archive::start_band() {
    // TODO(mbp): Make up the right real next name.
    BandWriter writer(this, _HARDCODED_SINGLE_BAND);
    writer.start();
    return writer;
}

} // namespace conserve

// vim: sw=4 et
