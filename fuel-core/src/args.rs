use crate::service::{Config, DbType, VMConfig};
use std::{env, io, net, path::PathBuf, string::ToString};
use structopt::StructOpt;
use strum::VariantNames;
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

    #[structopt(long = "db-type", default_value = "rocks-db", possible_values = &DbType::VARIANTS, case_insensitive = true)]
    pub database_type: DbType,

    /// Specify either an alias to a built-in configuration or filepath to a JSON file.
    #[structopt(name = "CHAIN_CONFIG", long = "chain", default_value = "local_testnet")]
    pub chain_config: String,

    /// Specify if backtraces are going to be traced in logs (Default false)
    #[structopt(long = "vm-backtrace")]
    pub vm_backtrace: bool,
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
            chain_config,
            vm_backtrace,
        } = self;

        let addr = net::SocketAddr::new(ip, port);

        Ok(Config {
            addr,
            database_path,
            database_type,
            chain_conf: chain_config.as_str().parse()?,
            vm: VMConfig {
                backtrace: vm_backtrace,
            },
        })
    }
}
