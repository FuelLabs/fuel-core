#![allow(unused_variables)]
use crate::{
    cli::DEFAULT_DB_PATH,
    FuelService,
};
use anyhow::{
    anyhow,
    Context,
};
use clap::Parser;
use fuel_chain_config::ChainConfig;
use fuel_core::service::{
    config::default_consensus_dev_key,
    Config,
    DbType,
    VMConfig,
};
use fuel_core_interfaces::{
    common::{
        fuel_tx::Address,
        prelude::SecretKey,
        secrecy::{
            ExposeSecret,
            Secret,
        },
    },
    model::SecretKeyWrapper,
};
use std::{
    env,
    net,
    ops::Deref,
    path::PathBuf,
    str::FromStr,
};
use strum::VariantNames;
use tracing::{
    info,
    log::warn,
    trace,
};

pub const CONSENSUS_KEY_ENV: &str = "CONSENSUS_KEY_SECRET";

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

    /// The signing key used when producing blocks.
    /// Setting via the `CONSENSUS_KEY_SECRET` ENV var is preferred.
    #[clap(long = "consensus-key")]
    pub consensus_key: Option<String>,

    /// Use a default insecure consensus key for testing purposes.
    /// This will not be enabled by default in the future.
    #[clap(long = "dev-keys", default_value = "true")]
    pub consensus_dev_key: bool,

    /// The block's fee recipient public key.
    ///
    /// If not set, `consensus_key` is used as the provider of the `Address`.
    #[clap(long = "coinbase-recipient")]
    pub coinbase_recipient: Option<String>,

    #[cfg(feature = "relayer")]
    #[clap(flatten)]
    pub relayer_args: relayer::RelayerArgs,

    #[cfg(feature = "p2p")]
    #[clap(flatten)]
    pub p2p_args: p2p::P2pArgs,

    #[clap(long = "metrics")]
    pub metrics: bool,
}

impl Command {
    pub fn get_config(self) -> anyhow::Result<Config> {
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
            consensus_key,
            consensus_dev_key,
            coinbase_recipient,
            #[cfg(feature = "relayer")]
            relayer_args,
            #[cfg(feature = "p2p")]
            p2p_args,
            metrics,
        } = self;

        let addr = net::SocketAddr::new(ip, port);

        #[cfg(feature = "p2p")]
        let p2p_cfg = {
            let mut p2p = match p2p_args.into() {
                Ok(value) => value,
                Err(e) => return Err(e),
            };
            p2p.metrics = metrics;
            p2p
        };

        let chain_conf: ChainConfig = chain_config.as_str().parse()?;
        // if consensus key is not configured, fallback to dev consensus key
        let consensus_key = load_consensus_key(consensus_key)?.or_else(|| {
            if consensus_dev_key {
                let key = default_consensus_dev_key();
                warn!(
                    "Fuel Core is using an insecure test key for consensus. Public key: {}",
                    key.public_key()
                );
                Some(Secret::new(key.into()))
            } else {
                // if consensus dev key is disabled, use no key
                None
            }
        });

        let coinbase_recipient = if let Some(coinbase_recipient) = coinbase_recipient {
            Address::from_str(coinbase_recipient.as_str()).map_err(|err| anyhow!(err))?
        } else {
            let consensus_key = consensus_key
                .as_ref()
                .cloned()
                .unwrap_or_else(|| Secret::new(SecretKeyWrapper::default()));

            let sk = consensus_key.expose_secret().deref();
            Address::from(*sk.public_key().hash())
        };

        Ok(Config {
            addr,
            database_path,
            database_type,
            chain_conf: chain_conf.clone(),
            utxo_validation,
            manual_blocks_enabled,
            vm: VMConfig {
                backtrace: vm_backtrace,
            },
            txpool: fuel_txpool::Config::new(chain_conf, min_gas_price, utxo_validation),
            block_importer: Default::default(),
            block_producer: fuel_block_producer::Config {
                utxo_validation,
                coinbase_recipient,
                metrics,
            },
            block_executor: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: relayer_args.into(),
            sync: Default::default(),
            #[cfg(feature = "p2p")]
            p2p: p2p_cfg,
            consensus_key,
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

// Attempt to load the consensus key from cli arg first, otherwise check the env.
fn load_consensus_key(
    cli_arg: Option<String>,
) -> anyhow::Result<Option<Secret<SecretKeyWrapper>>> {
    let secret_string = if let Some(cli_arg) = cli_arg {
        warn!("Consensus key configured insecurely using cli args. Consider setting the {} env var instead.", CONSENSUS_KEY_ENV);
        Some(cli_arg)
    } else {
        env::var(CONSENSUS_KEY_ENV).ok()
    };

    if let Some(key) = secret_string {
        let key =
            SecretKey::from_str(&key).context("failed to parse consensus signing key")?;
        Ok(Some(Secret::new(key.into())))
    } else {
        Ok(None)
    }
}
