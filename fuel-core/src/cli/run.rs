use crate::{
    cli::DEFAULT_DB_PATH,
    FuelService,
};
use clap::Parser;
use fuel_chain_config::ChainConfig;
use fuel_core::service::{
    Config,
    DbType,
    VMConfig,
};
use std::{
    env,
    io,
    net,
    path::PathBuf,
};
use strum::VariantNames;
use tracing::{
    info,
    trace,
};
#[cfg(feature = "p2p")]
mod p2p;

#[cfg(feature = "relayer")]
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

    #[cfg(feature = "relayer")]
    #[clap(flatten)]
    pub relayer_args: relayer::RelayerArgs,

    #[cfg(feature = "p2p")]
    #[clap(flatten)]
    pub p2p_args: p2p::P2pArgs,
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
            #[cfg(feature = "relayer")]
            relayer_args,
            #[cfg(feature = "p2p")]
            p2p_args,
        } = self;

        let addr = net::SocketAddr::new(ip, port);

        #[cfg(feature = "p2p")]
        let p2p = {
            match p2p_args.into() {
                Ok(value) => value,
                Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
            }
        };

        let chain_conf: ChainConfig = chain_config.as_str().parse()?;
        let consensus_params = chain_conf.transaction_parameters;

        Ok(Config {
            addr,
            database_path,
            database_type,
            chain_conf,
            utxo_validation,
            manual_blocks_enabled,
            vm: VMConfig {
                backtrace: vm_backtrace,
            },
            txpool: fuel_txpool::Config {
                min_gas_price,
                utxo_validation,
                ..Default::default()
            },
            block_importer: Default::default(),
            block_producer: fuel_block_producer::Config {
                utxo_validation,
                consensus_params,
            },
            block_executor: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: relayer_args.into(),
            sync: Default::default(),
            #[cfg(feature = "p2p")]
            p2p,
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
