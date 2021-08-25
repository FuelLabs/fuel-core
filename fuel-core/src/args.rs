use crate::service::{Config, DbType};
use std::path::PathBuf;
use std::string::ToString;
use std::{env, io, net};
use structopt::StructOpt;
use tracing_subscriber::filter::EnvFilter;

lazy_static::lazy_static! {
    pub static ref DEFAULT_DB_PATH: PathBuf = dirs::home_dir().unwrap().join(".fuel").join("db");
}

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(long = "ip", default_value = "127.0.0.1", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[structopt(long = "port", default_value = "4000")]
    pub port: u16,

    #[structopt(
        name = "DB_PATH",
        long = "db-path",
        parse(from_os_str),
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap()
    )]
    pub database_path: PathBuf,

    #[structopt(long = "db-type", default_value = "RocksDb")]
    pub database_type: DbType,
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
            database_type,
        } = self;

        let addr = net::SocketAddr::new(ip, port);

        Ok(Config {
            addr,
            database_path,
            database_type,
        })
    }
}
