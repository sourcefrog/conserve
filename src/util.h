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

using namespace boost::filesystem;

namespace proto {
    class Stamp;
    class Path;
};

void write_proto_to_file(
        const google::protobuf::Message& message,
        const boost::filesystem::path& path);

void read_proto_from_file(
        const boost::filesystem::path path,
        google::protobuf::Message* message);

std::string gethostname_str();

void break_path(
        const boost::filesystem::path &from_path,
        conserve::proto::Path *to_path_proto);

path unpack_path(const conserve::proto::Path &proto_path);

void populate_stamp(conserve::proto::Stamp *stamp);
}

// vim: sw=4 et
