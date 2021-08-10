use fuel_core::service::Config;
use std::path::PathBuf;
use std::{env, io, net};
use structopt::StructOpt;
use tracing_subscriber::filter::EnvFilter;

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(long = "ip", default_value = "127.0.0.1", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[structopt(long = "port", default_value = "4000")]
    pub port: u16,

    #[structopt(name = "DB_PATH", long = "db-path", parse(from_os_str))]
    pub database_path: Option<PathBuf>,
}

impl Opt {
    pub fn exec(self) -> io::Result<Config> {
        let filter = match env::var_os("RUST_LOG") {
            Some(_) => EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided"),
            None => EnvFilter::new("info"),
        };

        tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(std::io::stderr)
            .with_env_filter(filter)
            .init();

        let Opt {
            ip,
            port,
            database_path,
        } = self;

        let addr = net::SocketAddr::new(ip, port);

        Ok(Config {
            addr,
            database_path,
        })
    }
}
