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

#include <algorithm>

#include <boost/filesystem.hpp>
#include <boost/format.hpp>

#include <glog/logging.h>

#include "datareader.h"
#include "util.h"

namespace conserve {

using namespace std;
using namespace boost;
using namespace boost::filesystem;


DataReader::DataReader(path datafile_path) {
    int bzerror;
    file_ = fopen(datafile_path.c_str(), "rb");
    PCHECK(file_);
    bzfile_ = BZ2_bzReadOpen(&bzerror, file_, 1, 0, 0, 0);
    CHECK(bzfile_);
    CHECK(bzerror == BZ_OK);
}


DataReader::~DataReader() {
    int bzerror;
    BZ2_bzReadClose(&bzerror, bzfile_);
    PCHECK(!fclose(file_));
}


void DataReader::extract_to_fd(int64_t bytes_to_read, int to_fd) { 
    char buf[64 << 10];
    int bzerror;
    int bytes_read;
    for (; bytes_to_read > 0; bytes_to_read -= bytes_read) {
        int read_size = min((int64_t) sizeof buf, bytes_to_read);
        VLOG(1) << "try to read " << read_size;
        bytes_read = BZ2_bzRead(&bzerror, bzfile_, buf, read_size);
        VLOG(1) << "got " << bytes_read << " decompressed bytes";
        if (bzerror != BZ_OK && bzerror != BZ_STREAM_END) 
            LOG(FATAL) << "bzread failed: " 
                << BZ2_bzerror(bzfile_, &bzerror);
        if (!bytes_read)
            LOG(FATAL) << "bz2 stream ended early; still wanted "
                << bytes_to_read << " bytes";
        int written;
        for (int to_write = 0; to_write < bytes_read; to_write += written) {
            written = write(to_fd, &buf[to_write], bytes_read - to_write);
            PCHECK(written > 0);
        }
    }
}


} // namespace conserve

// vim: sw=4 et
