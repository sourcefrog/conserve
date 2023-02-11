// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

//! Terminal/text UI.

use std::fmt::Debug;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

use crate::progress::term::WriteToNutmeg;

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

/// Should progress be enabled for ad-hoc created Nutmeg views.
// (These should migrate to NUTMEG_VIEW.)
static PROGRESS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_tracing(
    time_style: &TraceTimeStyle,
    console_level: Level,
    json_path: &Option<PathBuf>,
) {
    use tracing_subscriber::fmt::time;
    fn hookup<FT>(timer: FT, console_level: Level, json_path: &Option<PathBuf>)
    where
        FT: FormatTime + Send + Sync + 'static,
    {
        let console_layer = tracing_subscriber::fmt::Layer::default()
            .with_ansi(clicolors_control::colors_enabled())
            .with_writer(WriteToNutmeg)
            .with_timer(timer)
            .with_filter(LevelFilter::from_level(console_level));
        // TODO: Maybe tracing_appender instead?
        let json_layer = json_path
            .as_ref()
            .map(|path| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .write(true)
                    .read(false)
                    .open(path)
                    .expect("open json log file")
            })
            .map(|w| {
                tracing_subscriber::fmt::Layer::default()
                    .json()
                    .with_writer(w)
            });
        Registry::default()
            .with(console_layer)
            .with(crate::trace_counter::CounterLayer())
            .with(json_layer)
            .init();
    }

    match time_style {
        TraceTimeStyle::None => hookup((), console_level, json_path),
        TraceTimeStyle::Utc => hookup(time::UtcTime::rfc_3339(), console_level, json_path),
        TraceTimeStyle::Relative => hookup(time::uptime(), console_level, json_path),
        TraceTimeStyle::Local => hookup(
            time::OffsetTime::local_rfc_3339().unwrap(),
            console_level,
            json_path,
        ),
    }
    trace!("Tracing enabled");
}

// #[deprecated]: Use the global view instead.
pub(crate) fn nutmeg_options() -> nutmeg::Options {
    nutmeg::Options::default().progress_enabled(PROGRESS_ENABLED.load(Ordering::Relaxed))
}
