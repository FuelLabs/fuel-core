use crate::cli::DEFAULT_DB_PATH;
use crate::FuelService;
use clap::Parser;
use fuel_core::config::{Config, DbType, VMConfig};
use std::{env, io, net, path::PathBuf};
use strum::VariantNames;
use tracing::{info, trace};

mod relayer;

#[derive(Debug, Clone, Parser)]
pub struct Command {
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

    /// Allows GraphQL Endpoints to arbitrarily advanced blocks. Should be used for local development only
    #[clap(long = "manual_blocks_enabled")]
    pub manual_blocks_enabled: bool,

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

    /// Enable predicate execution on transaction inputs.
    /// Will reject any transactions with predicates if set to false.
    #[clap(long = "predicates")]
    pub predicates: bool,

    #[clap(flatten)]
    pub relayer_args: relayer::RelayerArgs,
}

impl Command {
    pub fn get_config(self) -> io::Result<Config> {
        let Command {
            ip,
            port,
            database_path,
            database_type,
            chain_config,
            vm_backtrace,
            manual_blocks_enabled,
            utxo_validation,
            min_gas_price,
            predicates,
            relayer_args,
        } = self;

        let addr = net::SocketAddr::new(ip, port);
        Ok(Config {
            addr,
            database_path,
            database_type,
            chain_conf: chain_config.as_str().parse()?,
            utxo_validation,
            manual_blocks_enabled,
            vm: VMConfig {
                backtrace: vm_backtrace,
            },
            txpool: fuel_txpool::Config {
                min_gas_price,
                ..Default::default()
            },
            predicates,
            block_importer: Default::default(),
            block_producer: Default::default(),
            block_executor: Default::default(),
            relayer: relayer_args.into(),
            bft: Default::default(),
            sync: Default::default(),
        })
    }
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    let config = command.get_config()?;
    // log fuel-core version
    info!("Fuel Core version v{}", env!("CARGO_PKG_VERSION"));
    trace!("Initializing in TRACE mode.");
    // initialize the server
    let server = FuelService::new_node(config).await?;
    // pause the main task while service is running
    server.run().await;

    Ok(())
}
