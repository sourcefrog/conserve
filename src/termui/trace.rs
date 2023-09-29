// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

//! Terminal/text UI.

use std::fmt::Debug;
use std::fs::OpenOptions;
use std::path::PathBuf;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

/// Chosen style of timestamp prefix on trace lines.
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum TraceTimeStyle {
    /// No timestamp on trace lines.
    None,
    /// Universal time, in RFC 3339 style.
    Utc,
    /// Local time, in RFC 3339, using the offset when the program starts.
    Local,
    /// Time since the start of the process, in seconds.
    Relative,
}

#[must_use]
pub fn enable_tracing(
    monitor: &super::TermUiMonitor,
    time_style: &TraceTimeStyle,
    console_level: Level,
    json_path: &Option<PathBuf>,
) -> Option<WorkerGuard> {
    use tracing_subscriber::fmt::time;
    fn hookup<FT>(
        monitor: &super::TermUiMonitor,
        timer: FT,
        console_level: Level,
        json_path: &Option<PathBuf>,
    ) -> Option<WorkerGuard>
    where
        FT: FormatTime + Send + Sync + 'static,
    {
        let console_layer = tracing_subscriber::fmt::Layer::default()
            .with_ansi(clicolors_control::colors_enabled())
            .with_writer(monitor.view())
            .with_timer(timer)
            .with_filter(filter::Targets::new().with_target("conserve", console_level));
        let json_layer;
        let flush_guard;
        if let Some(json_path) = json_path {
            let file_writer = OpenOptions::new()
                .create(true)
                .append(true)
                .write(true)
                .read(false)
                .open(json_path)
                .expect("open json log file");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_writer);
            flush_guard = Some(guard);
            json_layer = Some(
                tracing_subscriber::fmt::Layer::default()
                    .json()
                    .with_writer(non_blocking),
            );
        } else {
            flush_guard = None;
            json_layer = None;
        }
        Registry::default()
            .with(console_layer)
            .with(crate::trace_counter::CounterLayer())
            .with(json_layer)
            .init();
        flush_guard
    }

    let flush_guard = match time_style {
        TraceTimeStyle::None => hookup(monitor, (), console_level, json_path),
        TraceTimeStyle::Utc => hookup(monitor, time::UtcTime::rfc_3339(), console_level, json_path),
        TraceTimeStyle::Relative => hookup(monitor, time::uptime(), console_level, json_path),
        TraceTimeStyle::Local => hookup(
            monitor,
            time::OffsetTime::local_rfc_3339().unwrap(),
            console_level,
            json_path,
        ),
    };
    trace!("Tracing enabled");
    flush_guard
}
