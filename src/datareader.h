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

#ifndef CONSERVE_DATAREADER_H
#define CONSERVE_DATAREADER_H

#include <stdio.h>
#include <bzlib.h>

#include "proto/conserve.pb.h"

#include "datareader.h"

namespace conserve {

using namespace boost::filesystem;

// TODO: Maybe not necessarily bzip.
class DataReader {
public:
    DataReader(path datafile_path);
    ~DataReader();

    void extract_to_fd(int64_t bytes_to_read, int to_fd);

private:
    path path_;
    BZFILE *bzfile_;
    FILE *file_;
};


} // namespace conserve

#endif // CONSERVE_DATAREADER_H

// vim: sw=4 et
