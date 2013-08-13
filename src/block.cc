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
#include "filecopy.h"
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
    data_file_ = fdopen(data_fd_, "w");
    PCHECK(data_file_);
    int bzerror;
    data_bzfile_ = BZ2_bzWriteOpen(&bzerror, data_file_, 9, 1, 0);
    CHECK(data_bzfile_);
}


void BlockWriter::copy_file_bz2(const path& source_path,
        int64_t* content_len)
{
    // TODO: Proper error handling, don't just abort.
    const size_t copy_buf_size = 64 << 10;
    char buf[copy_buf_size];
    int from_fd = open(source_path.c_str(), O_RDONLY);
    PCHECK(from_fd != -1);
    *content_len = 0;
    ssize_t bytes_read;
    int bzerror;
    while ((bytes_read = read(from_fd, buf, sizeof buf)) != 0) {
        PCHECK(bytes_read > 0);
        BZ2_bzWrite(&bzerror, data_bzfile_, buf, bytes_read);
        *content_len += bytes_read;
    }
    PCHECK(close(from_fd) == 0);
    // TODO: Accumulate the hash.
}


void BlockWriter::add_file(const path& source_path) {
    // TODO(mbp): Actually back up the files!
    int64_t content_len = -1;
    copy_file_bz2(source_path, &content_len);

    proto::FileIndex* file_index = index_proto_.add_file();
    file_index->set_path(source_path.string());
    CHECK(content_len >= 0);
    file_index->set_data_length(content_len);
}


void BlockWriter::finish() {
    int bzerror;
    BZ2_bzWriteClose(&bzerror, data_bzfile_, 0, 0, 0);
    PCHECK(!fclose(data_file_));

    populate_stamp(index_proto_.mutable_stamp());

    // TODO: Accumulate size and hash as we write the data file, and store it
    // into the index.
    index_proto_.set_compression(proto::BZIP2);
    write_proto_to_file(index_proto_, index_filename_);
    LOG(INFO) << "write block index in " << index_filename_;
}


bool Block::extract_filename_type(const string& f, char *out) {
    char f0 = tolower(f[0]);
    if (f0 == 'a' || f0 == 'd') {
        if (out)
            *out = f0;
        return true;
    } else
        return false;
}


bool Block::extract_block_number(const string& f, int* out) {
    char type;
    if (!extract_filename_type(f, &type))
        return false;
    for (int i = 1; i < f.size(); i++)
        if (!isdigit(f[i]))
            return false;
    if (out)
        *out = atoi(&f[1]);
    return true;
}


bool Block::resembles_index_filename(const string& f) {
    char ftype;
    return extract_filename_type(f, &ftype)
        && ftype == 'a'
        && extract_block_number(f, NULL);
}


bool Block::resembles_data_filename(const string& f) {
    char ftype;
    return extract_filename_type(f, &ftype)
        && ftype == 'd'
        && extract_block_number(f, NULL);
}


} // namespace conserve

// vim: sw=4 et
