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

namespace conserve {

void write_proto_to_file(
        const google::protobuf::Message& message,
        const boost::filesystem::path& path);

void read_proto_from_file(
        const boost::filesystem::path path,
        google::protobuf::Message* message);

std::string gethostname_str();

}

// vim: sw=4 et
