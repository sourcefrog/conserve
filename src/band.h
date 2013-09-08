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

#ifndef CONSERVE_BAND_H
#define CONSERVE_BAND_H

#include <boost/filesystem.hpp>

#include "block.h"

namespace conserve {

using namespace boost::filesystem;

class BlockWriter;
class BlockReader;

class Band {
public:
    static const string HEAD_NAME;
    static const string TAIL_NAME;

    path directory() { return band_directory_; }

protected:
    Band(Archive *archive, string name);
    Archive* archive_;
    string name_;
    path band_directory_;
    path head_file_name() const;
    path tail_file_name() const;
};


// Scans through a band in order.
class BandReader : public Band {
public:
    BandReader(Archive *archive, string name);

    BlockReader read_next_block();
    bool done() const;
    int current_block_number() const { return current_block_number_; };

private:
    int current_block_number_;
    proto::BandHead head_pb_;
    proto::BandTail tail_pb_;
};


// Holds an open writable band.
// Adding files to it creates new blocks.
// When all relevant files have been added, the band can be closed.
class BandWriter : public Band {
public:
    BandWriter(Archive *archive, string name);
    BlockWriter start_block();
    void start();
    void finish();

    int next_block_number();

private:
    int next_block_number_;
};

} // namespace conserve

#endif // CONSERVE_BAND_H

// vim: sw=4 et
