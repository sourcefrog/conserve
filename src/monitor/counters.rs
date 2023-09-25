//! Track counters.

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Counter {
    BandsDone,
    BandsTotal,
    FilesDone,
    IndexBytesDone,
    BlockBytesDone,
    BlockRead,
    BlockWrite,
    BlockMatchExisting,
    BlockCacheHit,
    // ...
}

impl Counter {
    pub(self) const COUNT: usize = 8;
}

#[derive(Default)]
pub(super) struct Counters {
    counters: [AtomicUsize; Counter::COUNT],
}

impl Counters {
    pub fn count(&self, counter: Counter, increment: usize) {
        self.counters[counter as usize].fetch_add(increment, Relaxed);
    }

    pub fn set(&self, counter: Counter, value: usize) {
        self.counters[counter as usize].store(value, Relaxed);
    }

    pub fn get(&self, counter: Counter) -> usize {
        self.counters[counter as usize].load(Relaxed)
    }
}
