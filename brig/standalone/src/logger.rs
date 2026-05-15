use log::{LevelFilter, Log};

struct PrintlnLogger;

impl Log for PrintlnLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        match record.level() {
            log::Level::Trace => eprint!("{}", record.args()),
            _ => println!("{}", record.args()),
        }
    }

    fn flush(&self) {}
}

pub(crate) fn init() {
    log::set_logger(&PrintlnLogger).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Trace);
}
