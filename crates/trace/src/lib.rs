#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

use ctor::ctor;
use std::env::var;
use tracing_subscriber::{
    fmt::format,
    EnvFilter,
};

#[ctor]
pub static TRACE: () = {
    if let Ok(v) = var("FUEL_TRACE") {
        match v.to_lowercase().as_str() {
            "1" | "true" | "on" => {
                let _ = tracing_subscriber::FmtSubscriber::builder()
                    .with_env_filter(EnvFilter::from_default_env())
                    .try_init();
            }
            "compact" => {
                let _ = tracing_subscriber::FmtSubscriber::builder()
                    .with_env_filter(EnvFilter::from_default_env())
                    .event_format(format().compact())
                    .try_init();
            }
            "pretty" => {
                let _ = tracing_subscriber::FmtSubscriber::builder()
                    .with_env_filter(EnvFilter::from_default_env())
                    .event_format(format().pretty())
                    .try_init();
            }
            "log-file" => {
                let log_path = var("FUEL_TRACE_PATH").unwrap_or_else(|_| {
                    concat!(env!("CARGO_MANIFEST_DIR"), "/logs").to_string()
                });
                let log_file = tracing_appender::rolling::daily(log_path, "logfile");

                let _ = tracing_subscriber::FmtSubscriber::builder()
                        .event_format(format().compact().pretty())
                        .with_ansi(false) // disabling terminal color fixes this issue: https://github.com/tokio-rs/tracing/issues/1817
                        .with_writer(log_file)
                        .try_init();
            }
            "log-show" => {
                use tracing_subscriber::prelude::*;
                let log_path = var("FUEL_TRACE_PATH").unwrap_or_else(|_| {
                    concat!(env!("CARGO_MANIFEST_DIR"), "/logs").to_string()
                });
                let log_file = tracing_appender::rolling::daily(log_path, "logfile");

                let log = tracing_subscriber::fmt::Layer::new()
                        .compact()
                        .pretty()
                        .with_ansi(false) // disabling terminal color fixes this issue: https://github.com/tokio-rs/tracing/issues/1817
                        .with_writer(log_file);

                let subscriber = tracing_subscriber::registry()
                    .with(EnvFilter::from_default_env())
                    .with(
                        tracing_subscriber::fmt::Layer::new()
                            .with_writer(std::io::stderr),
                    )
                    .with(log);
                let _ = subscriber.try_init();
            }
            _ => (),
        }
    }
};

#[macro_export]
macro_rules! enable_tracing {
    () => {
        static _TRACE: &$crate::TRACE<()> = &$crate::TRACE;
    };
}

#[cfg(test)]
mod tests {
    use tracing::*;

    #[test]
    fn works() {
        error!("I'm visible if FUEL_TRACE=1 is set");
        info!("I'm visible if FUEL_TRACE=1 and RUST_LOG=info are set");
    }
}
