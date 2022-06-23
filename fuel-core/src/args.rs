use anyhow::anyhow;
use clap::Parser;
use fuel_core::chain_config::{ChainConfig, StateConfig};
use fuel_core::database::Database;
use fuel_core::service::{Config, DbType, VMConfig};
use std::io::{self, Write};
use std::str::FromStr;
use std::{env, net, path::PathBuf};
use strum::VariantNames;
use tracing_subscriber::filter::EnvFilter;

lazy_static::lazy_static! {
    pub static ref DEFAULT_DB_PATH: PathBuf = dirs::home_dir().unwrap().join(".fuel").join("db");
}

pub const LOG_FILTER: &str = "RUST_LOG";
pub const HUMAN_LOGGING: &str = "HUMAN_LOGGING";

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "fuel-core",
    about = "Fuel client implementation",
    version,
    rename_all = "kebab-case"
)]
pub struct Opt {
    #[clap(subcommand)]
    pub _snapshot: Option<Snapshot>,

    #[clap(long = "ip", default_value = "127.0.0.1", parse(try_from_str))]
    pub ip: net::IpAddr,

    #[clap(long = "port", default_value = "4000")]
    pub port: u16,

    #[clap(
        name = "DB_PATH",
        long = "db-path",
        parse(from_os_str),
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap()
    )]
    pub database_path: PathBuf,

    #[clap(long = "db-type", default_value = "rocks-db", possible_values = &*DbType::VARIANTS, ignore_case = true)]
    pub database_type: DbType,

    /// Specify either an alias to a built-in configuration or filepath to a JSON file.
    #[clap(name = "CHAIN_CONFIG", long = "chain", default_value = "local_testnet")]
    pub chain_config: String,

    /// Enable logging of backtraces from vm errors
    #[clap(long = "vm-backtrace")]
    pub vm_backtrace: bool,

    /// Enable full utxo stateful validation
    /// disabled by default until downstream consumers stabilize
    #[clap(long = "utxo-validation")]
    pub utxo_validation: bool,

    /// The minimum allowed gas price
    #[clap(long = "min-gas-price", default_value = "0")]
    pub min_gas_price: u64,

    /// The minimum allowed byte price
    #[clap(long = "min-byte-price", default_value = "0")]
    pub min_byte_price: u64,

    /// Enable predicate execution on transaction inputs.
    /// Will reject any transactions with predicates if set to false.
    #[clap(long = "predicates")]
    pub predicates: bool,
}

// Not strictly neccessary, but makes it easy to add an export to .json option
#[derive(clap::Subcommand, Clone, Debug)]
pub enum Snapshot {
    Snapshot(SnapshotCommand),
}

impl Snapshot {
    pub fn get_args(self) -> Result<SnapshotCommand, anyhow::Error> {
        if let Snapshot::Snapshot(cmd) = self {
            Ok(cmd)
        } else {
            Err(anyhow!("No Arguments Provided"))
        }
    }
}

#[derive(Debug, Clone, Parser)]
pub struct SnapshotCommand {
    #[clap(
        name = "DB_PATH",
        long = "db-path",
        parse(from_os_str),
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap()
    )]
    pub database_path: PathBuf,

    /// Specify either an alias to a built-in configuration or filepath to a JSON file.
    #[clap(name = "CHAIN_CONFIG", long = "chain", default_value = "local_testnet")]
    pub chain_config: String,
}

impl Opt {
    pub fn get_config(self) -> io::Result<Config> {
        let filter = match env::var_os(LOG_FILTER) {
            Some(_) => EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided"),
            None => EnvFilter::new("info"),
        };

        let human_logging = env::var_os(HUMAN_LOGGING)
            .map(|s| {
                bool::from_str(s.to_str().unwrap())
                    .expect("Expected `true` or `false` to be provided for `HUMAN_LOGGING`")
            })
            .unwrap_or(true);

        let sub = tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(std::io::stderr)
            .with_env_filter(filter);

        if human_logging {
            // use pretty logs
            sub.with_ansi(true)
                .with_level(true)
                .with_line_number(true)
                .init();
        } else {
            // use machine parseable structured logs
            sub
                // disable terminal colors
                .with_ansi(false)
                .with_level(true)
                .with_line_number(true)
                // use json
                .json()
                .init();
        }

        let Opt {
            ip,
            port,
            database_path,
            database_type,
            chain_config,
            vm_backtrace,
            utxo_validation,
            min_gas_price,
            min_byte_price,
            predicates,
            _snapshot,
        } = self;

        let addr = net::SocketAddr::new(ip, port);
        Ok(Config {
            addr,
            database_path,
            database_type,
            chain_conf: chain_config.as_str().parse()?,
            utxo_validation,
            vm: VMConfig {
                backtrace: vm_backtrace,
            },
            tx_pool_config: fuel_txpool::Config {
                min_gas_price,
                min_byte_price,
                ..Default::default()
            },
            predicates,
        })
    }
}

pub fn dump_snapshot(path: PathBuf, config: ChainConfig) -> anyhow::Result<()> {
    let db = Database::open(&path)?;

    let state_conf = StateConfig::generate_state_config(db.clone());

    let chain_conf = ChainConfig {
        chain_name: config.chain_name,
        block_production: config.block_production,
        parent_network: config.parent_network,
        initial_state: Some(state_conf),
        transaction_parameters: config.transaction_parameters,
    };

    let serialized = serde_json::to_string(&chain_conf).unwrap();
    let mut stdout = io::stdout().lock();

    stdout.write_all(serialized.as_bytes())?;

    Ok(())
}
