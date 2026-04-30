use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(verbosity: u8) {
    let default = match verbosity {
        0 => "warn",
        1 => "info,cs=debug",
        _ => "debug,cs=trace",
    };
    let filter = EnvFilter::try_from_env("CS_LOG").unwrap_or_else(|_| EnvFilter::new(default));
    let layer = fmt::layer().with_target(false).with_writer(std::io::stderr);
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(layer)
        .try_init();
}
