// Copyright 2026 AsterSQL.
//! Standalone logger assembly. Only main installs the returned subscriber.
use astersql_pd_logutil::{LogConfig, LogLevelControl};
use std::{
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
};
use tracing::{Level, Subscriber};
use tracing_subscriber::{Layer, layer::SubscriberExt};

pub(crate) struct LogControl(AtomicU8);
fn threshold(level: &str) -> Result<u8, String> {
    match level.to_ascii_lowercase().as_str() {
        "debug" => Ok(0),
        "info" => Ok(1),
        "warn" | "warning" => Ok(2),
        "error" => Ok(3),
        "fatal" => Ok(4),
        _ => Err(format!("log level {level} is illegal")),
    }
}
impl LogLevelControl for LogControl {
    fn set_level(&self, level: &str) -> Result<(), String> {
        let next = threshold(level)?;
        self.0.store(next, Ordering::Release);
        tracing::warn!(target: "pd", level = level.to_ascii_lowercase(), "log level changed");
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct Writer(pub Arc<Mutex<Box<dyn Write + Send>>>);
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Writer {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}
pub(crate) fn subscriber(
    config: &LogConfig,
    writer: Writer,
) -> Result<(impl Subscriber + Send + Sync + use<>, Arc<LogControl>), crate::entry::ProcessError> {
    let control = Arc::new(LogControl(AtomicU8::new(threshold(&config.level)?)));
    let filter_control = control.clone();
    // Dependencies may log request URLs and credentials. This process sink only
    // accepts explicitly reviewed PD events with static messages/allowlisted fields.
    let filter = tracing_subscriber::filter::dynamic_filter_fn(move |metadata, _| {
        let rank = match *metadata.level() {
            Level::DEBUG => 0,
            Level::INFO => 1,
            Level::WARN => 2,
            Level::ERROR => 3,
            _ => return false,
        };
        metadata.target() == "pd" && rank >= filter_control.0.load(Ordering::Acquire)
    });
    let fmt = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(false)
        .with_ansi(false)
        .with_file(!config.disable_caller)
        .with_line_number(!config.disable_caller);
    let layer = match config.format.as_str() {
        "json" => {
            if config.disable_timestamp {
                fmt.json().without_time().boxed()
            } else {
                fmt.json().boxed()
            }
        }
        "text" | "console" | "" => {
            if config.disable_timestamp {
                fmt.without_time().boxed()
            } else {
                fmt.boxed()
            }
        }
        _ => return Err("unsupported log format".into()),
    };
    Ok((
        tracing_subscriber::registry().with(layer.with_filter(filter)),
        control,
    ))
}
pub(crate) fn init(config: &LogConfig) -> Result<Arc<LogControl>, crate::entry::ProcessError> {
    let writer: Box<dyn Write + Send> = if config.file.filename.is_empty() {
        Box::new(std::io::stdout())
    } else {
        let path = std::path::Path::new(&config.file.filename);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        Box::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        )
    };
    let (subscriber, control) = subscriber(config, Writer(Arc::new(Mutex::new(writer))))?;
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(control)
}

/// Configuration decoders may include raw source/value fragments in errors.
/// Only known value-free validation messages may cross the process boundary.
pub(crate) fn config_error(error: &str) -> &'static str {
    for message in [
        "-initial-cluster and -join can not be provided at the same time",
        "log directory shouldn't be the subdirectory of data directory",
        "local native mode requires both data directory and cluster ID",
        "local native cluster ID must be a nonzero integer",
        "local native data directory must not be empty",
        "local native mode requires classic PD without etcd or join settings",
        "local native mode requires literal loopback URLs with ports",
    ] {
        if error == message {
            return message;
        }
    }
    "pd-server: configuration could not be loaded or validated"
}
