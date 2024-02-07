use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config as LogConfig, Root},
    encode::pattern::PatternEncoder,
};

pub fn init_logger(log_file_path: &str) {
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} {l} {M} {m}{n}")))
        .build(log_file_path)
        .expect("Failed to create log file appender");

    let config = LogConfig::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(Root::builder().appender("logfile").build(LevelFilter::Info))
        .expect("Failed to build logger configuration");

    log4rs::init_config(config).expect("Failed to initialize logger");
}
