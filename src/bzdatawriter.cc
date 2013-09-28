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
#include <ctype.h>

#include <boost/filesystem.hpp>
#include <boost/format.hpp>

#include <glog/logging.h>

#include "bzdatawriter.h"

namespace conserve {

using namespace std;
using namespace boost;
using namespace boost::filesystem;

const size_t copy_buf_size = 64 << 10;


BzDataWriter::BzDataWriter(path data_filename) {
    path_ = data_filename;
    data_fd_ = open(data_filename.string().c_str(),
        O_CREAT|O_EXCL|O_WRONLY,
        0666);
    PCHECK(data_fd_ > 0);
    data_file_ = fdopen(data_fd_, "w");
    PCHECK(data_file_);
    int bzerror;
    bzfile_ = BZ2_bzWriteOpen(&bzerror, data_file_, 9, 1, 0);
    CHECK(bzfile_);
}


void BzDataWriter::store_file(const path& source_path,
        int64_t* content_len)
{
    // TODO: Proper error handling, don't just abort.
    char buf[copy_buf_size];
    int from_fd = open(source_path.c_str(), O_RDONLY);
    PCHECK(from_fd != -1);
    *content_len = 0;
    ssize_t bytes_read;
    int bzerror;
    while ((bytes_read = read(from_fd, buf, sizeof buf)) != 0) {
        PCHECK(bytes_read > 0);
        BZ2_bzWrite(&bzerror, bzfile_, buf, bytes_read);
        *content_len += bytes_read;
    }
    PCHECK(close(from_fd) == 0);
    // TODO: Accumulate and return the hash of the stored content.
}


BzDataWriter::~BzDataWriter() {
    int bzerror;
    BZ2_bzWriteClose(&bzerror, bzfile_, 0, 0, 0);
    PCHECK(!fclose(data_file_));
}


} // namespace conserve

// vim: sw=4 et
