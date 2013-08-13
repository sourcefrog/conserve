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

#ifndef CONSERVE_FILECOPY_H
#define CONSERVE_FILECOPY_H

namespace conserve {

using namespace boost::filesystem;

bool copy_file_contents(
    const path& from_path,
    int to_fd,
    char content_sha1[20],
    int64_t* content_length);

}; // namespace conserve

#endif // CONSERVE_FILECOPY_H

// vim: sw=4 et
