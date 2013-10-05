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

#ifndef CONSERVE_PROBLEM_H_
#define CONSERVE_PROBLEM_H_

#include <exception>
#include <string>
#include <boost/filesystem.hpp>

namespace conserve {

using namespace std;
using namespace boost::filesystem;

class Problem : public exception {
public:
    Problem(const string& object, const string& part,
            const string& result,
            const path& path, const string& os_error);

    virtual ~Problem() throw();

    virtual const char* what() const throw();

    string object_, part_, result_;
    path path_;
    string os_error_;

    // Logs a summary of this problem and then will either raise it
    // as an exception or return.
    void signal() const;

    // Return a string like "archive.header.unreadable"
    string shortform() const;

    // Write a description of this problem to the glog.
    void log() const;

private:
    // Precomposed full string for this exception.
    string what_;
};

} // namespace conserve

#endif // CONSERVE_PROBLEM_H_

// vim: sw=4 et
