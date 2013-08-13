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

#include <unistd.h>
#include <fcntl.h>
#include <sys/types.h>
#include <boost/filesystem.hpp>
#include <glog/logging.h>
#include <openssl/sha.h>

#include "filecopy.h"

namespace conserve {

using namespace boost::filesystem;

const size_t copy_buf_size = 64 << 10;

bool copy_file_contents(
    const path& from_path,
    int to_fd,
    char content_sha1[20],
    int64_t* content_length)
{
    // TODO: Proper error handling, don't just abort.
    char buf[copy_buf_size];
    int from_fd = open(from_path.c_str(), O_RDONLY);
    PCHECK(from_fd != -1);
    *content_length = 0;
    ssize_t bytes_read, bytes_written;
    while ((bytes_read = read(from_fd, buf, sizeof buf)) != 0) {
        PCHECK(bytes_read > 0);
        bytes_written = write(to_fd, buf, bytes_read);
        PCHECK(bytes_written > 0);
        CHECK(bytes_written == bytes_read);
        *content_length += bytes_read;
    }
    PCHECK(close(from_fd) == 0);
    // TODO: Accumulate the hash.
    return true;
}

}; // namespace conserve

// vim: sw=4 et
