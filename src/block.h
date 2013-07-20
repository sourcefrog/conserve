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

namespace conserve {

class BandWriter;

using namespace boost::filesystem;

class BlockWriter {
public:
	void start();
    void finish();
    BlockWriter(BandWriter band);

private:
    path block_directory_;
    int block_number_;
    path index_filename_;
    path data_filename_;
    int data_fd_;
};

} // namespace conserve

#endif // CONSERVE_BLOCK_H

// vim: sw=4 et
