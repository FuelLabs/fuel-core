use clap::arg_enum;
use structopt::StructOpt;
use tracing::Level;

use std::{io, net};

arg_enum! {
    #[derive(Debug)]
    pub enum Log {
        Error,
        Warn,
        Info,
        Debug,
        Trace,
    }
}

impl Into<Level> for Log {
    fn into(self) -> Level {
        match self {
            Log::Error => Level::ERROR,
            Log::Warn => Level::WARN,
            Log::Info => Level::INFO,
            Log::Debug => Level::DEBUG,
            Log::Trace => Level::TRACE,
        }
    }
}

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(long = "log-level", default_value = "info")]
    pub log_level: Log,

    #[structopt(long = "ip", default_value = "0.0.0.0", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[structopt(long = "port", default_value = "4000")]
    pub port: u16,
}

impl Opt {
    pub fn exec(self) -> io::Result<net::SocketAddr> {
        let Opt {
            log_level,
            ip,
            port,
        } = self;
        let log_level: tracing::Level = log_level.into();

        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(std::io::stderr)
            .with_max_level(log_level)
            .finish();

        tracing::subscriber::set_global_default(subscriber)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let addr = net::SocketAddr::new(ip, port);

        Ok(addr)
    }
}
