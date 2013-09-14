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

#ifndef CONSERVE_BLOCKREADER_H
#define CONSERVE_BLOCKREADER_H

#include <stdio.h>
#include <bzlib.h>

#include "proto/conserve.pb.h"

#include "datareader.h"

namespace conserve {

using namespace boost::filesystem;

class Block;
class DataReader;

class BlockReader : public Block {
public:
    BlockReader(path directory, int block_number);

    int file_number() const { return file_number_; }
    path file_path() const;
    void advance();
    bool done() const;
    const proto::FileIndex& file_index() const;
    
    // Restore the current file to the given output path.
    void restore_file(const path &restore_path);

private:
    DataReader data_reader_;
    proto::BlockIndex index_pb_;
    int file_number_;
};


} // namespace conserve

#endif // CONSERVE_BLOCKREADER_H

// vim: sw=4 et
