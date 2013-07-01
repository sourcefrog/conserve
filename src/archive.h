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

#ifndef CONSERVE_ARCHIVE_H_
#define CONSERVE_ARCHIVE_H_

#include "string"
#include <boost/filesystem.hpp>

namespace conserve {

using namespace std;

class BandWriter;

class Archive {
public:
    static Archive create(const string base_dir);

    Archive(const string base_dir) :
	base_dir_(base_dir)
	{}

    BandWriter start_band();

    const boost::filesystem::path base_dir_;

private:
};

} // namespace conserve
#endif // CONSERVE_ARCHIVE_H_
// vim: sw=4 et
