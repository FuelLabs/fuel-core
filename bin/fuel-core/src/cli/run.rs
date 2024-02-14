#![allow(unused_variables)]
use crate::{
    cli::{
        run::consensus::PoATriggerArgs,
        DEFAULT_DB_PATH,
    },
    FuelService,
};
use anyhow::{
    anyhow,
    Context,
};
use clap::Parser;
use fuel_core::{
    chain_config::{
        default_consensus_dev_key,
        ChainConfig,
    },
    producer::Config as ProducerConfig,
    service::{
        config::Trigger,
        Config,
        DbType,
        RelayerVerifierConfig,
        ServiceTrait,
        VMConfig,
    },
    txpool::Config as TxPoolConfig,
    types::{
        blockchain::primitives::SecretKeyWrapper,
        fuel_tx::ContractId,
        fuel_vm::SecretKey,
        secrecy::Secret,
    },
};
use pyroscope::{
    pyroscope::PyroscopeAgentRunning,
    PyroscopeAgent,
};
use pyroscope_pprofrs::{
    pprof_backend,
    PprofConfig,
};
use std::{
    env,
    net,
    path::PathBuf,
    str::FromStr,
};
use tracing::{
    info,
    trace,
    warn,
};

pub const CONSENSUS_KEY_ENV: &str = "CONSENSUS_KEY_SECRET";
// Default database cache is 1 GB
const DEFAULT_DATABASE_CACHE_SIZE: usize = 1024 * 1024 * 1024;

#[cfg(feature = "p2p")]
mod p2p;

mod consensus;
mod profiling;
#[cfg(feature = "relayer")]
mod relayer;

/// Run the Fuel client node locally.
#[derive(Debug, Clone, Parser)]
pub struct Command {
    #[clap(long = "ip", default_value = "127.0.0.1", value_parser, env)]
    pub ip: net::IpAddr,

    #[clap(long = "port", default_value = "4000", env)]
    pub port: u16,

    /// Vanity name for node, used in telemetry
    #[clap(long = "service-name", default_value = "fuel-core", value_parser, env)]
    pub service_name: String,

    /// The maximum database cache size in bytes.
    #[arg(
        long = "max-database-cache-size",
        default_value_t = DEFAULT_DATABASE_CACHE_SIZE,
        env
    )]
    pub max_database_cache_size: usize,

    #[clap(
        name = "DB_PATH",
        long = "db-path",
        value_parser,
        default_value = (*DEFAULT_DB_PATH).to_str().unwrap(),
        env
    )]
    pub database_path: PathBuf,

    #[clap(
        long = "db-type",
        default_value = "rocks-db",
        value_enum,
        ignore_case = true,
        env
    )]
    pub database_type: DbType,

    /// Specify either an alias to a built-in configuration or filepath to a JSON file.
    #[arg(
        name = "CHAIN_CONFIG",
        long = "chain",
        default_value = "local_testnet",
        env
    )]
    pub chain_config: String,

    /// Should be used for local development only. Enabling debug mode:
    /// - Allows GraphQL Endpoints to arbitrarily advance blocks.
    /// - Enables debugger GraphQL Endpoints.
    /// - Allows setting `utxo_validation` to `false`.
    #[arg(long = "debug", env)]
    pub debug: bool,

    /// Enable logging of backtraces from vm errors
    #[arg(long = "vm-backtrace", env)]
    pub vm_backtrace: bool,

    /// Enable full utxo stateful validation
    /// disabled by default until downstream consumers stabilize
    #[arg(long = "utxo-validation", env)]
    pub utxo_validation: bool,

    /// The minimum allowed gas price
    #[arg(long = "min-gas-price", default_value = "0", env)]
    pub min_gas_price: u64,

    /// The signing key used when producing blocks.
    /// Setting via the `CONSENSUS_KEY_SECRET` ENV var is preferred.
    #[arg(long = "consensus-key", env)]
    pub consensus_key: Option<String>,

    /// A new block is produced instantly when transactions are available.
    #[clap(flatten)]
    pub poa_trigger: PoATriggerArgs,

    /// Use a default insecure consensus key for testing purposes.
    /// This will not be enabled by default in the future.
    #[arg(long = "dev-keys", default_value = "true", env)]
    pub consensus_dev_key: bool,

    /// The block's fee recipient public key.
    ///
    /// If not set, `consensus_key` is used as the provider of the `Address`.
    #[arg(long = "coinbase-recipient", env)]
    pub coinbase_recipient: Option<String>,

    #[cfg_attr(feature = "relayer", clap(flatten))]
    #[cfg(feature = "relayer")]
    pub relayer_args: relayer::RelayerArgs,

    #[cfg_attr(feature = "p2p", clap(flatten))]
    #[cfg(feature = "p2p")]
    pub p2p_args: p2p::P2PArgs,

    #[cfg_attr(feature = "p2p", clap(flatten))]
    #[cfg(feature = "p2p")]
    pub sync_args: p2p::SyncArgs,

    #[arg(long = "metrics", env)]
    pub metrics: bool,

    #[clap(long = "verify-max-da-lag", default_value = "10", env)]
    pub max_da_lag: u64,

    #[clap(long = "verify-max-relayer-wait", default_value = "30s", env)]
    pub max_wait_time: humantime::Duration,

    /// The max time to live of the transaction inside of the `TxPool`.
    #[clap(long = "tx-pool-ttl", default_value = "5m", env)]
    pub tx_pool_ttl: humantime::Duration,

    /// The max number of transactions that the `TxPool` can simultaneously store.
    #[clap(long = "tx-max-number", default_value = "4064", env)]
    pub tx_max_number: usize,

    /// The max depth of the dependent transactions that supported by the `TxPool`.
    #[clap(long = "tx-max-depth", default_value = "10", env)]
    pub tx_max_depth: usize,

    /// The maximum number of active subscriptions that supported by the `TxPool`.
    #[clap(long = "tx-number-active-subscriptions", default_value = "4064", env)]
    pub tx_number_active_subscriptions: usize,

    /// The number of reserved peers to connect to before starting to sync.
    #[clap(long = "min-connected-reserved-peers", default_value = "0", env)]
    pub min_connected_reserved_peers: usize,

    /// Time to wait after receiving the latest block before considered to be Synced.
    #[clap(long = "time-until-synced", default_value = "0s", env)]
    pub time_until_synced: humantime::Duration,

    /// Time to wait after submitting a query before debug info will be logged about query.
    #[clap(long = "query-log-threshold-time", default_value = "2s", env)]
    pub query_log_threshold_time: humantime::Duration,

    /// Timeout before drop the request.
    #[clap(long = "api-request-timeout", default_value = "30m", env)]
    pub api_request_timeout: humantime::Duration,

    #[clap(flatten)]
    pub profiling: profiling::ProfilingArgs,
}

impl Command {
    pub fn get_config(self) -> anyhow::Result<Config> {
        let Command {
            ip,
            port,
            service_name: name,
            max_database_cache_size,
            database_path,
            database_type,
            chain_config,
            vm_backtrace,
            debug,
            utxo_validation,
            min_gas_price,
            consensus_key,
            poa_trigger,
            consensus_dev_key,
            coinbase_recipient,
            #[cfg(feature = "relayer")]
            relayer_args,
            #[cfg(feature = "p2p")]
            p2p_args,
            #[cfg(feature = "p2p")]
            sync_args,
            metrics,
            max_da_lag,
            max_wait_time,
            tx_pool_ttl,
            tx_max_number,
            tx_max_depth,
            tx_number_active_subscriptions,
            min_connected_reserved_peers,
            time_until_synced,
            query_log_threshold_time,
            api_request_timeout,
            profiling: _,
        } = self;

        let addr = net::SocketAddr::new(ip, port);

        let chain_conf: ChainConfig = chain_config.as_str().parse()?;

        #[cfg(feature = "relayer")]
        let relayer_cfg = relayer_args.into_config();

        #[cfg(feature = "p2p")]
        let p2p_cfg = p2p_args.into_config(chain_conf.chain_name.clone(), metrics)?;

        let trigger: Trigger = poa_trigger.into();

        if trigger != Trigger::Never {
            info!("Block production mode: {:?}", &trigger);
        } else {
            info!("Block production disabled");
        }

        // if consensus key is not configured, fallback to dev consensus key
        let consensus_key = load_consensus_key(consensus_key)?.or_else(|| {
            if consensus_dev_key && trigger != Trigger::Never {
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

        if consensus_key.is_some() && trigger == Trigger::Never {
            warn!("Consensus key configured but block production is disabled!")
        }

        let coinbase_recipient = if let Some(coinbase_recipient) = coinbase_recipient {
            Some(
                ContractId::from_str(coinbase_recipient.as_str())
                    .map_err(|err| anyhow!(err))?,
            )
        } else {
            tracing::warn!("The coinbase recipient `ContractId` is not set!");
            None
        };

        let verifier = RelayerVerifierConfig {
            max_da_lag: max_da_lag.into(),
            max_wait_time: max_wait_time.into(),
        };

        let config = Config {
            addr,
            api_request_timeout: api_request_timeout.into(),
            max_database_cache_size,
            database_path,
            database_type,
            chain_conf: chain_conf.clone(),
            debug,
            utxo_validation,
            block_production: trigger,
            vm: VMConfig {
                backtrace: vm_backtrace,
            },
            txpool: TxPoolConfig::new(
                tx_max_number,
                tx_max_depth,
                chain_conf,
                min_gas_price,
                utxo_validation,
                metrics,
                tx_pool_ttl.into(),
                tx_number_active_subscriptions,
            ),
            block_producer: ProducerConfig {
                utxo_validation,
                coinbase_recipient,
                metrics,
            },
            block_executor: Default::default(),
            block_importer: Default::default(),
            #[cfg(feature = "relayer")]
            relayer: relayer_cfg,
            #[cfg(feature = "p2p")]
            p2p: p2p_cfg,
            #[cfg(feature = "p2p")]
            sync: sync_args.into(),
            consensus_key,
            name,
            verifier,
            min_connected_reserved_peers,
            time_until_synced: time_until_synced.into(),
            query_log_threshold_time: query_log_threshold_time.into(),
        };
        Ok(config)
    }
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    let profiling = command.profiling.clone();
    let config = command.get_config()?;

    // start profiling agent if url is configured
    let _profiling_agent = start_pyroscope_agent(profiling, &config)?;

    // log fuel-core version
    info!("Fuel Core version v{}", env!("CARGO_PKG_VERSION"));
    trace!("Initializing in TRACE mode.");
    // initialize the server
    let server = FuelService::new_node(config).await?;
    // pause the main task while service is running
    tokio::select! {
        result = server.await_stop() => {
            result?;
        }
        _ = shutdown_signal() => {}
    }

    server.stop_and_await().await?;

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

fn start_pyroscope_agent(
    profiling_args: profiling::ProfilingArgs,
    config: &Config,
) -> anyhow::Result<Option<PyroscopeAgent<PyroscopeAgentRunning>>> {
    profiling_args
        .pyroscope_url
        .as_ref()
        .map(|url| -> anyhow::Result<_> {
            // Configure profiling backend
            let agent = PyroscopeAgent::builder(url, &"fuel-core".to_string())
                .tags(vec![
                    ("service", config.name.as_str()),
                    ("network", config.chain_conf.chain_name.as_str()),
                ])
                .backend(pprof_backend(
                    PprofConfig::new().sample_rate(profiling_args.pprof_sample_rate),
                ))
                .build()
                .context("failed to start profiler")?;
            let agent_running = agent.start().unwrap();
            Ok(agent_running)
        })
        .transpose()
}

async fn shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("sigterm received");
            }
            _ = sigint.recv() => {
                tracing::info!("sigint received");
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        tracing::info!("CTRL+C received");
    }
    Ok(())
}
