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

#include "proto/conserve.pb.h"

namespace conserve {

using namespace boost::filesystem;

class Block {
public:
    static bool resembles_index_filename(const string&);
    static bool resembles_data_filename(const string&);
    static bool extract_filename_type(const string&, char*);
    static bool extract_block_number(const string&, int*);

    Block(path directory, int block_number);

    path index_path() const { return index_path_; };

protected:
    path block_directory_;
    int block_number_;
    path index_path_;
    path data_filename_;
};


} // namespace conserve

#endif // CONSERVE_BLOCK_H

// vim: sw=4 et
