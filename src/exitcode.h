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

#ifndef CONSERVE_EXITCODE_H_
#define CONSERVE_EXITCODE_H_

namespace conserve {

enum ExitCode {
    EXIT_OK = 0,
    EXIT_DIFFERENCES = 1,
    EXIT_PROBLEMS_NOTED = 2,
    EXIT_PROBLEMS_STOPPED = 3,
    EXIT_COMMAND_LINE = 4
};

}

#endif // CONSERVE_EXITCODE_H_

// vim: sw=4 et
