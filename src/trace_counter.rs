// Copyright 2023 Martin Pool.

//! Count the number of `tracing` errors and warnings.

use std::sync::atomic::{AtomicUsize, Ordering};

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// Count of errors emitted to trace.
static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Count of warnings emitted to trace.
static WARN_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Return the number of errors logged in the program so far.
pub fn global_error_count() -> usize {
    ERROR_COUNT.load(Ordering::Relaxed)
}

/// Return the number of warnings logged in the program so far.
pub fn global_warn_count() -> usize {
    WARN_COUNT.load(Ordering::Relaxed)
}

/// A tracing Layer that counts errors and warnings into static counters.
pub(crate) struct CounterLayer();

impl<S> Layer<S> for CounterLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        match *event.metadata().level() {
            Level::ERROR => ERROR_COUNT.fetch_add(1, Ordering::Relaxed),
            Level::WARN => WARN_COUNT.fetch_add(1, Ordering::Relaxed),
            _ => 0,
        };
    }
}
