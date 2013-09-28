// Conserve - robust backup system
// Copyright 2013 Martin Pool
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

#ifndef CONSERVE_BZDATAWRITER_H
#define CONSERVE_BZDATAWRITER_H

#include <stdio.h>
#include <bzlib.h>

namespace conserve {

using namespace boost::filesystem;

class BzDataWriter {
public:
    BzDataWriter(path datafile_path);
    ~BzDataWriter();

    void store_file(const path& source_path, int64_t* content_len);

private:
    int data_fd_;
    path path_;
    BZFILE *bzfile_;
    FILE *data_file_;
};


} // namespace conserve

#endif // CONSERVE_BZDATAWRITER_H

// vim: sw=4 et
