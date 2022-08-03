use std::path::PathBuf;

use tracing::metadata::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;
use tracing_subscriber::fmt;

pub struct LoggingOptions {
    pub file: Option<PathBuf>,
    pub level: tracing::Level,
}

pub fn init(options: LoggingOptions) -> std::result::Result<LogGuard, String> {
    let subscriber = Registry::default()
        .with(
            fmt::Layer::default()
                .with_target(false)
                // FIXME: Don't pipe directly into stdout if we got a progress bar.
                .with_writer(std::io::stdout)
                .with_filter(LevelFilter::from(options.level))
        );

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_| "Failed to update global default logger".to_string())?;

    Ok(LogGuard{ })
}

/// Guards all logging activity.
/// When dropping the pending logs will be written synchronously
/// and all open handles closed.
pub struct LogGuard {

}