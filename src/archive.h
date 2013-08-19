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

#ifndef CONSERVE_ARCHIVE_H_
#define CONSERVE_ARCHIVE_H_

#include "string"
#include <boost/filesystem.hpp>

namespace conserve {

using namespace std;
using namespace boost::filesystem;

class BandWriter;

class Archive {
public:
    static Archive create(const path& base_dir);

    Archive(const path& base_dir) :
	base_dir_(base_dir)
	{}

    string last_band_name();
    BandWriter start_band();

    static const string HEAD_NAME;

    const path base_dir_;

private:
    // TODO: Obviously, support multiple bands.
    static const string _HARDCODED_SINGLE_BAND;
};

} // namespace conserve
#endif // CONSERVE_ARCHIVE_H_
// vim: sw=4 et
