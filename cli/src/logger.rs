use std::io::stderr;
use std::sync::OnceLock;

use log::set_logger;
use log::set_max_level;
use log::LevelFilter;
use log::Log;
use log::Metadata;
use log::Record;
use log::SetLoggerError;

pub struct Logger;

impl Logger {
    pub fn init(verbose: bool) -> Result<(), SetLoggerError> {
        let max_level = if verbose {
            LevelFilter::Trace
        } else {
            LevelFilter::Warn
        };
        set_logger(LOGGER.get_or_init(move || Logger)).map(|()| set_max_level(max_level))
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        use std::fmt::Write;
        let mut buffer = String::with_capacity(4096);
        if writeln!(&mut buffer, "{} {}", record.level(), record.args()).is_ok() {
            use std::io::Write;
            let _ = stderr().write_all(buffer.as_bytes());
        }
    }

    fn flush(&self) {
        use std::io::Write;
        let _ = stderr().flush();
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();
