use std::fs::File;
use tracing_subscriber::EnvFilter;

pub fn init_logging(level: &str) {
    let file = File::create("gluck.log").ok();
    if let Some(file) = file {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level)),
            )
            .with_writer(file)
            .with_ansi(false)
            .init();
    }
}
