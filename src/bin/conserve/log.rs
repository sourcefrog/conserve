use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::ops::Deref;

use lazy_static::lazy_static;
use tracing::Subscriber;
use tracing::metadata::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;
use tracing_subscriber::fmt;

struct TerminalWriter { }

impl TerminalWriter { }

lazy_static!{
    pub static ref TERMINAL_OUTPUT: Mutex<Option<Arc<Mutex<dyn Write + Send + Sync>>>> = Mutex::new(
        Some(Arc::new(Mutex::new(std::io::stdout())))        
    );
}

impl Write for TerminalWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let current_target = TERMINAL_OUTPUT.lock().expect("lock() should not fail");
        if let Some(target) = current_target.deref() {
            let mut target = target.lock().expect("lock() should not fail");
            target.write(buf)
        } else {
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let output = TERMINAL_OUTPUT.lock().expect("lock() should not fail");
        if let Some(target) = output.deref() {
            let mut target = target.lock().expect("lock() should not fail");
            target.flush()
        } else {
            Ok(())
        }
    }
}

pub struct LoggingOptions {
    pub file: Option<PathBuf>,
    pub level: tracing::Level,
    pub terminal_raw: bool,
}

pub fn init(options: LoggingOptions) -> std::result::Result<LogGuard, String> {
    let mut worker_guard = None;
    let registry = Registry::default();
  
    // Terminal logger.
    // TODO: Enable timestamps except when in raw mode.
    //       Right now this can't be achived since without_time updates the struct signature...
    let registry = registry.with(
        fmt::Layer::default()
                .without_time()
                .with_level(!options.terminal_raw)
                .with_target(false)
                .with_writer(|| TerminalWriter{})
                .with_filter(LevelFilter::from(options.level))
    );

    // File logger.
    let registry: Box<dyn Subscriber + Send + Sync + 'static> = if let Some(path) = options.file {
        let directory = path.parent()
            .ok_or("can't resolve log file directory")?;

        let file_name = path.file_name()
            .ok_or("can't get log file name")?
            .to_string_lossy()
            .to_string();

        let writer = tracing_appender::rolling::never(directory, file_name);
        let (writer, guard) = tracing_appender::non_blocking(writer);
        worker_guard = Some(guard);

        Box::new(
            registry.with(
                fmt::Layer::default()
                        .with_ansi(false)
                        .with_target(false)
                        .with_writer(writer)
                        .with_filter(LevelFilter::from(options.level))
            )
        )
    } else {
        Box::new(registry)
    };

    tracing::subscriber::set_global_default(registry)
        .map_err(|_| "Failed to update global default logger".to_string())?;

    Ok(LogGuard{ _worker_guard: worker_guard })
}

/// Guards all logging activity.
/// When dropping the pending logs will be written synchronously
/// and all open handles closed.
pub struct LogGuard {
    _worker_guard: Option<WorkerGuard>,
}

pub struct ViewLogGuard {
    released: bool,
    previous_logger: Option<Arc<Mutex<dyn Write + Send + Sync>>>,
}

impl ViewLogGuard {
    fn restore_previous(&mut self) {
        if self.released {
            return;
        }

        self.released = true;
        
        let mut output = TERMINAL_OUTPUT.lock().unwrap();
        *output = self.previous_logger.take();
    }
}

impl Drop for ViewLogGuard {
    fn drop(&mut self) {
        self.restore_previous();
    }
}

pub fn update_terminal_target(target: Arc<Mutex<dyn Write + Send + Sync>>) -> ViewLogGuard {
    let mut output = TERMINAL_OUTPUT.lock().unwrap();
    let previous_logger = output.replace(target);

    ViewLogGuard { previous_logger, released: false }
}