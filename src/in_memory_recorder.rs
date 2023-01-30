// Copyright 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! A metrics recorder that just keeps atomic values in memory,
//! so they can be logged or inspected at the end of the process, or potentially
//! earlier.

use std::collections::BTreeMap;
use std::sync::{atomic::Ordering, Arc};

use lazy_static::lazy_static;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, SharedString, Unit};
use metrics_util::registry::{AtomicStorage, Registry};
use tracing::debug;

lazy_static! {
    static ref REGISTRY: Registry<Key, AtomicStorage> = Registry::atomic();
}

pub struct InMemory {}
pub static IN_MEMORY: InMemory = InMemory {};

impl Recorder for InMemory {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        todo!()
    }

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        todo!()
    }

    fn describe_histogram(&self, __key: KeyName, __unit: Option<Unit>, _description: SharedString) {
        todo!()
    }

    fn register_counter(&self, key: &Key) -> Counter {
        REGISTRY.get_or_create_counter(key, |c| Counter::from_arc(Arc::clone(c)))
    }

    fn register_gauge(&self, _key: &Key) -> Gauge {
        todo!()
    }

    fn register_histogram(&self, _key: &Key) -> Histogram {
        todo!()
    }
}

pub fn counter_values() -> BTreeMap<String, u64> {
    REGISTRY
        .get_counter_handles()
        .into_iter()
        .map(|(key, counter)| (key.name().to_owned(), counter.load(Ordering::Relaxed)))
        .collect()
}

pub fn emit_to_trace() {
    for (counter_name, count) in counter_values() {
        debug!(counter_name, count);
    }
}
