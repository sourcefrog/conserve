// Copyright 2023 Martin Pool

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Terminal UI: tracing, progress bars, etc.

mod monitor;
mod trace;

pub use monitor::TermUiMonitor;
pub use trace::{TraceTimeStyle, enable_tracing};
