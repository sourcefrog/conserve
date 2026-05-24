// Conserve backup system.
// Copyright 2015-2026 Martin Pool.

//! Terminal/text UI.

use std::fmt::Debug;
use std::fs::OpenOptions;
use std::path::PathBuf;

use tracing::{Level, info, trace};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    Registry, filter,
    fmt::{format::Writer, time},
    layer::Layer,
    prelude::*,
};

use crate::termui::TermUiMonitor;

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
    term_ui_monitor: &TermUiMonitor,
    time_style: &TraceTimeStyle,
    console_level: Level,
    json_path: &Option<PathBuf>,
    trace_tmp: bool,
) -> Option<WorkerGuard> {
    let time_style = time_style.clone();
    let console_layer = tracing_subscriber::fmt::Layer::default()
        .with_ansi(clicolors_control::colors_enabled())
        .with_writer(term_ui_monitor.view())
        .with_timer(time_style)
        .with_filter(filter::Targets::new().with_target("conserve", console_level));
    let json_layer;
    let (trace_tmp_layer, tmp_path) = if trace_tmp {
        let (tmp_file, tmp_path) = tempfile::Builder::new()
            .prefix(&format!("conserve-trace-{}-", std::process::id()))
            .suffix(".log")
            .tempfile()
            .expect("create trace temp file")
            .keep()
            .expect("keep trace temp file");
        let layer = tracing_subscriber::fmt::Layer::default()
            .with_writer(tmp_file)
            .with_ansi(false)
            .with_timer(time::uptime())
            .with_filter(filter::Targets::new().with_target("conserve", Level::TRACE));
        (Some(layer), Some(tmp_path))
    } else {
        (None, None)
    };
    let flush_guard;
    if let Some(json_path) = json_path {
        let file_writer = OpenOptions::new()
            .create(true)
            .append(true)
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
        .with(json_layer)
        .with(trace_tmp_layer)
        .init();

    trace!("Tracing enabled");
    if let Some(tmp_path) = tmp_path {
        info!("Trace temp file: {}", tmp_path.display());
    }
    flush_guard
}

impl time::FormatTime for TraceTimeStyle {
    fn format_time(&self, w: &mut Writer) -> std::fmt::Result {
        match self {
            TraceTimeStyle::None => Ok(()),
            TraceTimeStyle::Utc => time::UtcTime::rfc_3339().format_time(w),
            TraceTimeStyle::Relative => time::uptime().format_time(w),
            TraceTimeStyle::Local => time::OffsetTime::local_rfc_3339().unwrap().format_time(w),
        }
    }
}
