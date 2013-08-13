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

#ifndef CONSERVE_BLOCK_H
#define CONSERVE_BLOCK_H

#include <stdio.h>
#include <bzlib.h>

#include "proto/conserve.pb.h"

namespace conserve {

class BandWriter;

using namespace boost::filesystem;

class Block {
public:
    static bool resembles_index_filename(const string&);
    static bool resembles_data_filename(const string&);
    static bool extract_filename_type(const string&, char*);
    static bool extract_block_number(const string&, int*);
};


class BlockWriter {
public:
    void start();
    void finish();
    BlockWriter(BandWriter band);

    void add_file(const path&);

private:
    path block_directory_;
    int block_number_;
    path index_filename_;
    path data_filename_;
    int data_fd_;

    FILE* data_file_;
    BZFILE* data_bzfile_;

    // Accumulates index entries as files are added.
    conserve::proto::BlockIndex index_proto_;

    void copy_file_bz2(const path& source_path, int64_t* content_len);
};

} // namespace conserve

#endif // CONSERVE_BLOCK_H

// vim: sw=4 et
