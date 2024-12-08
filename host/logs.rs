use crate::*;

use chrono::NaiveDate;
use rev_buf_reader::RevBufReader;
use std::{
    fs::OpenOptions,
    io::{BufRead, Read, Write},
    path::PathBuf,
};
use tracing::Level;
use tracing_appender::{non_blocking::WorkerGuard, rolling::RollingFileAppender};
pub use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{
    filter::Targets,
    fmt::{self, time::ChronoUtc},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

pub const LOGS_INFO_NAME: &str = "info";
pub const LOGS_TRACES_NAME: &str = "traces";
pub const TRACES_DATE_FORMAT: &str = "%Y-%m-%d";

state!(LOGS: Logs = {
    let AppConfig {
        data_dir,
        ..
    } = APP_CONFIG.check();

    let mut info_path = data_dir.clone();
    info_path.push(LOGS_INFO_NAME);

    let mut traces_path = data_dir.clone();
    traces_path.push(LOGS_TRACES_NAME);

    Logs {
        info: Log(info_path),
        traces: Log(traces_path),
    }
});

/// Holds path to the log file
pub struct Log(pub PathBuf);

/// Holds paths to the log files
pub struct Logs {
    pub info: Log,
    pub traces: Log,
}

impl Logs {
    pub fn latest_info(&self, offset: usize, count: usize) -> Vec<String> {
        let Ok(file) = OpenOptions::new().read(true).open(&self.info.0) else {
            return vec![];
        };
        let buf = RevBufReader::new(file);
        buf.lines()
            .skip(offset)
            .take(count)
            .map(|l| l.expect("Could not parse line"))
            .collect()
    }

    pub fn traces(&self, date: NaiveDate) -> String {
        let path = format!("{}/{}", self.traces.0.display(), date.format(TRACES_DATE_FORMAT));
        let Ok(mut file) = OpenOptions::new().read(true).open(path) else {
            return "Failed to open traces file".into();
        };
        let mut entries = String::new();
        if let Err(e) = file.read_to_string(&mut entries) {
            return format!("Failed to read traces file: {e}");
        };
        entries
        // if entries.len() > 1 {
        //     // not empty
        //     entries.pop(); // remove last newline
        //     entries.pop(); // remove last comma
        // }

        // entries.push_str("]");
    }

    pub fn recorded_traces_dates(&self) -> Vec<NaiveDate> {
        let mut res = vec![];
        let paths = std::fs::read_dir(&self.traces.0).unwrap();
        for path in paths {
            if let Ok(entry) = path {
                let pathbuf = entry.path();
                let filename = pathbuf.file_name().unwrap().to_string_lossy();
                if let Ok(date) = NaiveDate::parse_from_str(&filename, TRACES_DATE_FORMAT) {
                    res.push(date);
                }
            }
        }
    res
    }
}

struct LogWriter;
impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let Ok(log) = std::str::from_utf8(buf) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not UTF-8 log",
            ));
        };

        let Ok(log) = ansi_to_html::convert(log) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not ANSI log",
            ));
        };

        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&LOGS.info.0)
            .expect("Unable to open log for writing");

        f.write_all(log.as_bytes())
            .expect(&format!("Unable to append info log: {log}"));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct MakeInfoLogWriter;
impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for MakeInfoLogWriter {
    type Writer = LogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        LogWriter
    }
}

fn info_filter(level: LevelFilter) -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env()
        .unwrap()
        .add_directive("sqlparser::parser=info".parse().unwrap())
        .add_directive("sqlparser::dialect=info".parse().unwrap())
        .add_directive("tower_sessions_core=info".parse().unwrap())
        .add_directive("h2=info".parse().unwrap())
        .add_directive("hyper=info".parse().unwrap())
        .add_directive("rustls=info".parse().unwrap())
        .add_directive("reqwest=info".parse().unwrap())
        .add_directive("russh=info".parse().unwrap())
        .add_directive("sled=info".parse().unwrap())
}

fn traces_filter() -> Targets {
    Targets::new()
        .with_target("sled::tree", Level::INFO)
        .with_target("sled::context", Level::DEBUG)
        .with_target("sled::pagecache", Level::INFO)
        .with_target("sqlparser::parser", Level::INFO)
        .with_target("sqlparser::dialect", Level::INFO)
        .with_target("prest::host::traces", Level::DEBUG)
        .with_target("async_io", Level::DEBUG)
        .with_target("polling", Level::DEBUG)
        .with_target("russh", Level::INFO)
        .with_target("rustls_acme", Level::INFO)
        .with_default(LevelFilter::TRACE)
}

struct AppenderWithCommas {
    inner: RollingFileAppender,
}

impl Write for AppenderWithCommas {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let Ok(log) = std::str::from_utf8(buf) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not UTF-8 log",
            ));
        };
        let mut line = log.to_owned();
        line.pop(); // remove newline
        line.push_str(",\n"); // add comma and newline
        self.inner.write(line.as_bytes())?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Initializes log collection and writing into files
pub fn init_tracing_subscriber() -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily(LOGS.traces.0.clone(), "");
    let appender_with_commas = AppenderWithCommas {
        inner: file_appender,
    };
    let (non_blocking, guard) = tracing_appender::non_blocking(appender_with_commas);

    let traces_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_filter(traces_filter());

    let info_layer = fmt::layer()
        .with_timer(ChronoUtc::new("%m/%d %k:%M:%S".to_owned()))
        .with_file(false)
        .with_target(false)
        .map_writer(move |_| MakeInfoLogWriter)
        .with_filter(info_filter(LevelFilter::INFO));

    #[cfg(debug_assertions)]
    let shell_layer = fmt::layer()
        .with_timer(ChronoUtc::new("%k:%M:%S".to_owned()))
        .with_filter(info_filter(LevelFilter::DEBUG));

    let registry = tracing_subscriber::registry()
        .with(traces_layer)
        .with(info_layer);

    #[cfg(debug_assertions)]
    let registry = registry.with(shell_layer);

    registry.init();

    guard
}
