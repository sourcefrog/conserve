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
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;
use std::sync::{atomic::Ordering, Arc};

use ::metrics::{
    Counter, Gauge, Histogram, HistogramFn, Key, KeyName, Recorder, SharedString, Unit,
};
use itertools::Itertools;
use lazy_static::lazy_static;
use metrics_util::registry::{Registry, Storage};
use metrics_util::Summary;
use serde_json::json;
use tracing::debug;

use crate::{Error, Result};

lazy_static! {
    static ref REGISTRY: Registry<Key, SummaryStorage> = Registry::new(SummaryStorage::new());
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

    fn register_histogram(&self, key: &Key) -> Histogram {
        REGISTRY.get_or_create_histogram(key, |g| Histogram::from_arc(Arc::clone(g)))
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
    for (histogram_name, histogram) in REGISTRY
        .get_histogram_handles()
        .into_iter()
        .sorted_by_key(|(k, _v)| k.clone())
    {
        let summary = histogram.0.lock().unwrap();
        debug!(
            histogram = histogram_name.name(),
            p10 = summary.quantile(0.1),
            p50 = summary.quantile(0.5),
            p90 = summary.quantile(0.9),
            p99 = summary.quantile(0.99),
            p100 = summary.quantile(1.0),
        );
    }
}

/// Like AtomicStorage but using a Summary.
struct SummaryStorage {}

impl SummaryStorage {
    const fn new() -> Self {
        SummaryStorage {}
    }
}

impl<K> Storage<K> for SummaryStorage {
    type Counter = Arc<AtomicU64>;
    type Gauge = Arc<AtomicU64>;
    type Histogram = Arc<SummaryHistogram>;

    fn counter(&self, _key: &K) -> Self::Counter {
        Arc::new(AtomicU64::new(0))
    }

    fn gauge(&self, _: &K) -> Self::Gauge {
        Arc::new(AtomicU64::new(0))
    }

    fn histogram(&self, _: &K) -> Self::Histogram {
        Arc::new(SummaryHistogram::new())
    }
}

struct SummaryHistogram(Mutex<Summary>);

impl HistogramFn for SummaryHistogram {
    fn record(&self, value: f64) {
        self.0.lock().unwrap().add(value)
    }
}

impl SummaryHistogram {
    fn new() -> Self {
        SummaryHistogram(Mutex::new(Summary::with_defaults()))
    }
}

pub fn write_json_metrics(path: &Path) -> Result<()> {
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let j = json!( {
        "counters": counter_values(),
    });
    serde_json::to_writer_pretty(f, &j).map_err(|source| Error::SerializeJson {
        path: path.to_string_lossy().to_string(),
        source,
    })
}
