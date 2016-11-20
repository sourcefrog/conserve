// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

/// Generic UI trait.

use super::Report;

pub mod terminal;

pub trait UI {
    fn show_progress(&self, &Report);
}
