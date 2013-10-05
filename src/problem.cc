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

#include <string>

#include <glog/logging.h>

#include "problem.h"

namespace conserve {

using namespace boost;

Problem::Problem(const string& object, const string& part,
        const string& result,
        const path& path,
        const string& os_error) :
    object_(object), part_(part), result_(result),
    path_(path), os_error_(os_error)
{
}


void Problem::log() const {
    LOG(ERROR) << "Problem: " << shortform()
        << (!path_.empty() ? ": " + path_.string() : "")
        << (!os_error_.empty() ? ": " + os_error_ : "");
}


void Problem::signal() const {
    log();
    throw this;
}


string Problem::shortform() const {
    return object_ + "." + part_ + "." + result_;
}


} // namespace conserve

// vim: sw=4 et
