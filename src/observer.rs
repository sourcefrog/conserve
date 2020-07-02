// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Callback that observes long-running operations.

use crate::*;

/// Observes results of archive validation.
pub trait ValidateObserver {
    fn error(&mut self, error: Error);
}

/// Collects all messages about archive validation.
#[derive(Debug, Default)]
pub struct ValidateCollectObserver {
    pub errors: Vec<Error>,
}

impl ValidateObserver for ValidateCollectObserver {
    fn error(&mut self, error: Error) {
        self.errors.push(error)
    }
}

/// Prints errors/warnings during validation to the UI.
#[derive(Debug, Default)]
pub struct ValidateUiObserver {}

impl ValidateObserver for ValidateUiObserver {
    fn error(&mut self, error: Error) {
        ui::problem(&format!("{}", error));
    }
}
