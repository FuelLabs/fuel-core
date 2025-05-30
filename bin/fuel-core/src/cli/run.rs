#![allow(unused_variables)]

use crate::{
    FuelService,
    cli::{
        ShutdownListener,
        default_db_path,
        run::{
            consensus::PoATriggerArgs,
            graphql::GraphQLArgs,
            tx_pool::TxPoolArgs,
            tx_status_manager::TxStatusManagerArgs,
        },
    },
};
use anyhow::Context;
use clap::Parser;
use fuel_core::{
    chain_config::default_consensus_dev_key,
    combined_database::{
        CombinedDatabase,
        CombinedDatabaseConfig,
    },
    fuel_core_graphql_api::{
        Costs,
        ServiceConfig as GraphQLConfig,
    },
    producer::Config as ProducerConfig,
    service::{
        Config,
        DbType,
        RelayerConsensusConfig,
        config::{
            DaCompressionMode,
            Trigger,
        },
        genesis::NotifyCancel,
    },
    state::rocks_db::{
        ColumnsPolicy,
        DatabaseConfig,
    },
    tx_status_manager::config::Config as TxStatusManagerConfig,
    txpool::config::{
        BlackList,
        Config as TxPoolConfig,
        HeavyWorkConfig,
        PoolLimits,
        ServiceChannelLimits,
    },
    types::{
        fuel_tx::ContractId,
        fuel_vm::SecretKey,
        secrecy::Secret,
    },
};

use fuel_core_chain_config::{
    SnapshotMetadata,
    SnapshotReader,
};
use fuel_core_metrics::config::{
    DisableConfig,
    Module,
};
use fuel_core_types::{
    blockchain::header::StateTransitionBytecodeVersion,
    signer::SignMode,
};
use pyroscope::{
    PyroscopeAgent,
    pyroscope::PyroscopeAgentRunning,
};
use pyroscope_pprofrs::{
    PprofConfig,
    pprof_backend,
};
use rlimit::{
    Resource,
    getrlimit,
};
use std::{
    env,
    net,
    num::{
        NonZeroU32,
        NonZeroU64,
    },
    path::PathBuf,
    str::FromStr,
    time::Duration,
};
use tracing::{
    info,
    trace,
    warn,
};

#[cfg(feature = "rocksdb")]
use fuel_core::state::historical_rocksdb::StateRewindPolicy;

use crate::cli::run::gas_price::GasPriceArgs;
use fuel_core::service::config::GasPriceConfig;
#[cfg(feature = "parallel-executor")]
use std::num::NonZeroUsize;

#[cfg(feature = "p2p")]
mod p2p;

#[cfg(feature = "shared-sequencer")]
mod shared_sequencer;

mod consensus;
mod gas_price;
mod graphql;
#[cfg(feature = "p2p")]
mod preconfirmation_signature_service;
mod profiling;
#[cfg(feature = "relayer")]
mod relayer;
mod tx_pool;
mod tx_status_manager;

/// Run the Fuel client node locally.
#[derive(Debug, Clone, Parser)]
pub struct Command {
    /// Vanity name for node, used in telemetry
    #[clap(long = "service-name", default_value = "fuel-core", value_parser, env)]
    pub service_name: String,

    /// The maximum database cache size in bytes.
    #[arg(long = "max-database-cache-size", env)]
    pub max_database_cache_size: Option<usize>,

    #[clap(
        name = "DB_PATH",
        long = "db-path",
        value_parser,
        default_value = default_db_path().into_os_string(),
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

    #[cfg(feature = "rocksdb")]
    /// Defines a specific number of file descriptors that RocksDB can use.
    ///
    /// If defined as -1 no limit will be applied and will use the OS limits.
    /// If not defined the system default divided by two is used.
    #[clap(
        long = "rocksdb-max-fds",
        env,
        default_value = get_default_max_fds().to_string()
    )]
    pub rocksdb_max_fds: i32,

    #[cfg(feature = "rocksdb")]
    /// Defines the state rewind policy for the database when RocksDB is enabled.
    ///
    /// The duration defines how many blocks back the rewind feature works.
    /// Assuming each block requires one second to produce.
    ///
    /// The default value is 7 days = 604800 blocks.
    ///
    /// The `BlockHeight` is `u32`, meaning the maximum possible number of blocks
    /// is less than 137 years. `2^32 / 24 / 60 / 60 / 365` = `136.1925195332` years.
    /// If the value is 136 years or more, the rewind feature is enabled for all blocks.
    #[clap(long = "state-rewind-duration", default_value = "7d", env)]
    pub state_rewind_duration: humantime::Duration,

    /// Snapshot from which to do (re)genesis. Defaults to local testnet configuration.
    #[arg(name = "SNAPSHOT", long = "snapshot", env)]
    pub snapshot: Option<PathBuf>,

    /// Prunes the db. Genesis is done from the provided snapshot or the local testnet
    /// configuration.
    #[arg(name = "DB_PRUNE", long = "db-prune", env, default_value = "false")]
    pub db_prune: bool,

    /// The determines whether to continue the services on internal error or not.
    #[clap(long = "continue-services-on-error", default_value = "false", env)]
    pub continue_on_error: bool,

    /// Should be used for local development only. Enabling debug mode:
    /// - Allows GraphQL Endpoints to arbitrarily advance blocks.
    /// - Enables debugger GraphQL Endpoints.
    /// - Allows setting `utxo_validation` to `false`.
    #[arg(long = "debug", env)]
    pub debug: bool,

    /// Allows execution of transactions based on past block, such as:
    /// - Dry run in the past
    #[arg(long = "historical-execution", env)]
    pub historical_execution: bool,

    /// Enable logging of backtraces from vm errors
    #[arg(long = "vm-backtrace", env)]
    #[deprecated]
    pub vm_backtrace: bool,

    /// Enable full utxo stateful validation
    /// disabled by default until downstream consumers stabilize
    #[arg(long = "utxo-validation", env)]
    pub utxo_validation: bool,

    /// Overrides the version of the native executor.
    #[arg(long = "native-executor-version", env)]
    pub native_executor_version: Option<StateTransitionBytecodeVersion>,

    /// Number of cores to use for the parallel executor.
    #[cfg(feature = "parallel-executor")]
    #[arg(long = "executor-number-of-cores", env, default_value = "1")]
    pub executor_number_of_cores: NonZeroUsize,

    /// All the configurations for the gas price service.
    #[clap(flatten)]
    pub gas_price: gas_price::GasPriceArgs,

    /// The signing key used when producing blocks.
    /// Setting via the `CONSENSUS_KEY_SECRET` ENV var is preferred.
    #[arg(long = "consensus-key", env = "CONSENSUS_KEY_SECRET")]
    pub consensus_key: Option<String>,

    /// Use [AWS KMS](https://docs.aws.amazon.com/kms/latest/APIReference/Welcome.html)for signing blocks.
    /// Loads the AWS credentials and configuration from the environment.
    /// Takes key_id as an argument, e.g. key ARN works.
    #[arg(long = "consensus-aws-kms", env, conflicts_with = "consensus_key")]
    #[cfg(feature = "aws-kms")]
    pub consensus_aws_kms: Option<String>,

    /// If given, the node will produce and store da-compressed blocks
    /// with the given retention time.
    #[arg(long = "da-compression", env)]
    pub da_compression: Option<humantime::Duration>,

    /// The starting height of the compression service.
    #[arg(long = "da-compression-starting-height", env)]
    pub da_compression_starting_height: Option<NonZeroU32>,

    /// A new block is produced instantly when transactions are available.
    #[clap(flatten)]
    pub poa_trigger: PoATriggerArgs,

    /// The path to the directory containing JSON encoded predefined blocks.
    #[arg(long = "predefined-blocks-path", env)]
    pub predefined_blocks_path: Option<PathBuf>,

    /// The block's fee recipient public key.
    ///
    /// If not set, `consensus_key` is used as the provider of the `Address`.
    #[arg(long = "coinbase-recipient", env)]
    pub coinbase_recipient: Option<ContractId>,

    /// The cli arguments supported by the `TxPool`.
    #[clap(flatten)]
    pub tx_pool: TxPoolArgs,

    /// The cli arguments supported by the `Tx Status Manager`.
    #[clap(flatten)]
    pub tx_status_manager: TxStatusManagerArgs,

    /// The cli arguments supported by the GraphQL API service.
    #[clap(flatten)]
    pub graphql: GraphQLArgs,

    #[cfg_attr(feature = "relayer", clap(flatten))]
    #[cfg(feature = "relayer")]
    pub relayer_args: relayer::RelayerArgs,

    #[cfg_attr(feature = "p2p", clap(flatten))]
    #[cfg(feature = "p2p")]
    pub p2p_args: p2p::P2PArgs,

    #[cfg_attr(feature = "p2p", clap(flatten))]
    #[cfg(feature = "p2p")]
    pub sync_args: p2p::SyncArgs,

    #[cfg_attr(feature = "p2p", clap(flatten))]
    #[cfg(feature = "p2p")]
    pub pre_confirmation_signature_service_args:
        preconfirmation_signature_service::PreconfirmationArgs,

    #[cfg_attr(feature = "shared-sequencer", clap(flatten))]
    #[cfg(feature = "shared-sequencer")]
    pub shared_sequencer_args: shared_sequencer::Args,

    #[arg(long = "disable-metrics", value_delimiter = ',', help = fuel_core_metrics::config::help_string(), env = "DISABLE_METRICS")]
    pub metrics: Vec<Module>,

    #[clap(long = "verify-max-da-lag", default_value = "10", env)]
    pub max_da_lag: u64,

    #[clap(long = "verify-max-relayer-wait", default_value = "30s", env)]
    pub max_wait_time: humantime::Duration,

    /// The number of reserved peers to connect to before starting to sync.
    #[clap(long = "min-connected-reserved-peers", default_value = "0", env)]
    pub min_connected_reserved_peers: usize,

    /// Time to wait after receiving the latest block before considered to be Synced.
    #[clap(long = "time-until-synced", default_value = "0s", env)]
    pub time_until_synced: humantime::Duration,

    /// The timeout after which the production of a block is considered failed.
    #[clap(long = "production-timeout", default_value = "20s", env)]
    pub production_timeout: humantime::Duration,

    /// The size of the memory pool in number of `MemoryInstance`s.
    #[clap(long = "memory-pool-size", default_value = "32", env)]
    pub memory_pool_size: usize,

    #[clap(flatten)]
    pub profiling: profiling::ProfilingArgs,
}

impl Command {
    pub async fn get_config(self) -> anyhow::Result<Config> {
        #[allow(deprecated)]
        let Command {
            service_name: name,
            max_database_cache_size,
            database_path,
            database_type,
            #[cfg(feature = "rocksdb")]
            rocksdb_max_fds,
            #[cfg(feature = "rocksdb")]
            state_rewind_duration,
            db_prune,
            snapshot,
            continue_on_error,
            vm_backtrace: _,
            debug,
            historical_execution,
            utxo_validation,
            native_executor_version,
            #[cfg(feature = "parallel-executor")]
            executor_number_of_cores,
            gas_price,
            consensus_key,
            #[cfg(feature = "aws-kms")]
            consensus_aws_kms,
            da_compression,
            da_compression_starting_height,
            poa_trigger,
            predefined_blocks_path,
            coinbase_recipient,
            #[cfg(feature = "relayer")]
            relayer_args,
            #[cfg(feature = "p2p")]
            p2p_args,
            #[cfg(feature = "p2p")]
            sync_args,
            #[cfg(feature = "p2p")]
            pre_confirmation_signature_service_args,
            #[cfg(feature = "shared-sequencer")]
            shared_sequencer_args,
            metrics,
            max_da_lag,
            max_wait_time,
            tx_pool,
            tx_status_manager,
            graphql,
            min_connected_reserved_peers,
            time_until_synced,
            production_timeout,
            memory_pool_size,
            profiling: _,
        } = self;

        let GasPriceArgs {
            starting_gas_price,
            gas_price_change_percent,
            min_gas_price,
            gas_price_threshold_percent,
            min_da_gas_price,
            max_da_gas_price,
            da_gas_price_p_component,
            da_gas_price_d_component,
            da_committer_url,
            da_poll_interval,
            da_starting_recorded_height,
        } = gas_price;

        let enabled_metrics = metrics.list_of_enabled();

        if !enabled_metrics.is_empty() {
            info!("`{:?}` metrics are enabled", enabled_metrics);
        } else {
            info!("All metrics are disabled");
        }

        if gas_price.max_da_gas_price < gas_price.min_da_gas_price {
            anyhow::bail!(
                "The maximum DA gas price must be greater than or equal to the minimum DA gas price"
            );
        }

        let addr = net::SocketAddr::new(graphql.ip, graphql.port);

        let snapshot_reader = match snapshot.as_ref() {
            None => crate::cli::local_testnet_reader(),
            Some(path) => {
                let metadata = SnapshotMetadata::read(path)?;
                SnapshotReader::open(metadata)?
            }
        };
        let chain_config = snapshot_reader.chain_config();

        #[cfg(feature = "relayer")]
        let relayer_cfg = relayer_args.into_config();

        #[cfg(feature = "p2p")]
        let p2p_cfg = p2p_args.into_config(
            chain_config.chain_name.clone(),
            metrics.is_enabled(Module::P2P),
        )?;

        #[cfg(feature = "p2p")]
        let preconfirmation_signature_service_config =
            fuel_core_poa::pre_confirmation_signature_service::config::Config {
                key_rotation_interval: *pre_confirmation_signature_service_args
                    .key_rotation_interval,
                key_expiration_interval: *pre_confirmation_signature_service_args
                    .key_expiration_interval,
                echo_delegation_interval: *pre_confirmation_signature_service_args
                    .echo_delegation_interval,
            };

        let trigger: Trigger = poa_trigger.into();

        if trigger != Trigger::Never {
            info!("Block production mode: {:?}", &trigger);
        } else {
            info!("Block production disabled");
        }

        let mut consensus_signer = SignMode::Unavailable;

        #[cfg(feature = "aws-kms")]
        if let Some(key_id) = consensus_aws_kms {
            let config = aws_config::load_from_env().await;
            let client = aws_sdk_kms::Client::new(&config);
            // Get the public key, and ensure that the signing key
            // is accessible and has the correct type.
            let key = client.get_public_key().key_id(&key_id).send().await?;
            if key.key_spec != Some(aws_sdk_kms::types::KeySpec::EccSecgP256K1) {
                anyhow::bail!("The key is not of the correct type, got {:?}", key);
            }
            let Some(cached_public_key) = key.public_key() else {
                anyhow::bail!("AWS KMS did not return a public key when requested");
            };
            let cached_public_key_bytes = cached_public_key.clone().into_inner();
            consensus_signer = SignMode::Kms {
                key_id,
                client,
                cached_public_key_bytes,
            };
        }

        if matches!(consensus_signer, SignMode::Unavailable) {
            if let Some(consensus_key) = consensus_key {
                let key = SecretKey::from_str(&consensus_key)
                    .context("failed to parse consensus signing key")?;
                consensus_signer = SignMode::Key(Secret::new(key.into()));
            } else if debug {
                // if consensus key is not configured, fallback to dev consensus key
                let key = default_consensus_dev_key();
                warn!(
                    "Fuel Core is using an insecure test key for consensus. Public key: {}, SecretKey: {}",
                    key.public_key(),
                    key
                );
                consensus_signer = SignMode::Key(Secret::new(key.into()));
            }
        };

        match consensus_signer.address() {
            Ok(signer_address) => {
                if let Some(address) = signer_address {
                    info!(
                        "Consensus signer is specified and its address is {}",
                        address
                    );
                }
            }
            _ => {
                warn!(
                    "Consensus Signer is specified but it was not possible to retrieve its address"
                );
            }
        };

        if consensus_signer.is_available() && trigger == Trigger::Never {
            warn!("Consensus key configured but block production is disabled!");
        }

        let coinbase_recipient = if let Some(coinbase_recipient) = coinbase_recipient {
            Some(coinbase_recipient)
        } else {
            tracing::warn!("The coinbase recipient `ContractId` is not set!");
            None
        };

        let verifier = RelayerConsensusConfig {
            max_da_lag: max_da_lag.into(),
            max_wait_time: max_wait_time.into(),
        };

        #[cfg(feature = "rocksdb")]
        let state_rewind_policy = {
            if database_type != DbType::RocksDb {
                tracing::warn!("State rewind policy is only supported with RocksDB");
            }

            let blocks = state_rewind_duration.as_secs();

            if blocks == 0 {
                StateRewindPolicy::NoRewind
            } else {
                let maximum_blocks: humantime::Duration = "136y".parse()?;

                if blocks >= maximum_blocks.as_secs() {
                    StateRewindPolicy::RewindFullRange
                } else {
                    StateRewindPolicy::RewindRange {
                        size: NonZeroU64::new(blocks)
                            .expect("The value is not zero above"),
                    }
                }
            }
        };

        let combined_db_config = CombinedDatabaseConfig {
            database_path,
            database_type,
            #[cfg(feature = "rocksdb")]
            database_config: DatabaseConfig {
                max_fds: rocksdb_max_fds,
                cache_capacity: max_database_cache_size,
                #[cfg(feature = "production")]
                columns_policy: ColumnsPolicy::OnCreation,
                #[cfg(not(feature = "production"))]
                columns_policy: ColumnsPolicy::Lazy,
            },
            #[cfg(feature = "rocksdb")]
            state_rewind_policy,
        };

        let block_importer = fuel_core::service::config::fuel_core_importer::Config::new(
            metrics.is_enabled(Module::Importer),
        );

        let gas_price_metrics = metrics.is_enabled(Module::GasPrice);

        let da_compression = match da_compression {
            Some(retention_duration) => DaCompressionMode::Enabled(
                fuel_core::service::config::DaCompressionConfig {
                    retention_duration: retention_duration.into(),
                    starting_height: da_compression_starting_height,
                    metrics: metrics.is_enabled(Module::Compression),
                },
            ),
            None => DaCompressionMode::Disabled,
        };

        let TxPoolArgs {
            tx_pool_ttl,
            tx_ttl_check_interval,
            tx_max_number,
            tx_max_total_bytes,
            tx_max_total_gas,
            tx_max_chain_count,
            tx_blacklist_addresses,
            tx_blacklist_coins,
            tx_blacklist_messages,
            tx_blacklist_contracts,
            tx_number_threads_to_verify_transactions,
            tx_size_of_verification_queue,
            tx_number_threads_p2p_sync,
            tx_size_of_p2p_sync_queue,
            tx_max_pending_read_requests,
            tx_max_pending_write_requests,
            tx_pending_pool_ttl,
            tx_pending_pool_size_percentage,
        } = tx_pool;

        let TxStatusManagerArgs {
            tx_number_active_subscriptions,
            status_cache_ttl,
        } = tx_status_manager;

        let black_list = BlackList::new(
            tx_blacklist_addresses,
            tx_blacklist_coins,
            tx_blacklist_messages,
            tx_blacklist_contracts,
        );

        let pool_limits = PoolLimits {
            max_txs: tx_max_number,
            max_gas: tx_max_total_gas,
            max_bytes_size: tx_max_total_bytes,
        };

        let pool_heavy_work_config = HeavyWorkConfig {
            number_threads_to_verify_transactions:
                tx_number_threads_to_verify_transactions,
            size_of_verification_queue: tx_size_of_verification_queue,
            number_threads_p2p_sync: tx_number_threads_p2p_sync,
            size_of_p2p_sync_queue: tx_size_of_p2p_sync_queue,
        };

        let service_channel_limits = ServiceChannelLimits {
            max_pending_read_pool_requests: tx_max_pending_read_requests,
            max_pending_write_pool_requests: tx_max_pending_write_requests,
        };

        let gas_price_config = GasPriceConfig {
            starting_exec_gas_price: gas_price.starting_gas_price,
            exec_gas_price_change_percent: gas_price.gas_price_change_percent,
            min_exec_gas_price: gas_price.min_gas_price,
            exec_gas_price_threshold_percent: gas_price.gas_price_threshold_percent,
            da_gas_price_factor: NonZeroU64::new(100).expect("100 is not zero"),
            starting_recorded_height: da_starting_recorded_height,
            min_da_gas_price,
            max_da_gas_price,
            max_da_gas_price_change_percent: gas_price.gas_price_change_percent,
            da_gas_price_p_component,
            da_gas_price_d_component,
            gas_price_metrics,
            activity_normal_range_size: 100,
            activity_capped_range_size: 0,
            activity_decrease_range_size: 0,
            da_committer_url,
            block_activity_threshold: 0,
            da_poll_interval: gas_price.da_poll_interval.map(Into::into),
        };

        let tx_pool_ttl: Duration = tx_pool_ttl.into();
        let subscription_ttl =
            // The connection should be closed automatically after the `SqueezedOut` event.
            // But because of slow/malicious consumers, the subscriber can still be occupied.
            // We allow the subscriber to receive the event produced by TxPool's TTL.
            // But we still want to drop subscribers after `2 * TxPool_TTL`.
            tx_pool_ttl.saturating_mul(2);

        let config = Config {
            graphql_config: GraphQLConfig {
                addr,
                number_of_threads: graphql.graphql_number_of_threads,
                database_batch_size: graphql.database_batch_size,
                max_queries_depth: graphql.graphql_max_depth,
                max_queries_complexity: graphql.graphql_max_complexity,
                max_queries_recursive_depth: graphql.graphql_max_recursive_depth,
                max_queries_resolver_recursive_depth: graphql
                    .max_queries_resolver_recursive_depth,
                max_queries_directives: graphql.max_queries_directives,
                max_concurrent_queries: graphql.graphql_max_concurrent_queries,
                request_body_bytes_limit: graphql.graphql_request_body_bytes_limit,
                api_request_timeout: graphql.api_request_timeout.into(),
                assemble_tx_dry_run_limit: graphql.assemble_tx_dry_run_limit,
                assemble_tx_estimate_predicates_limit: graphql
                    .assemble_tx_estimate_predicates_limit,
                query_log_threshold_time: graphql.query_log_threshold_time.into(),
                costs: Costs {
                    balance_query: graphql.costs.balance_query,
                    coins_to_spend: graphql.costs.coins_to_spend,
                    get_peers: graphql.costs.get_peers,
                    estimate_predicates: graphql.costs.estimate_predicates,
                    assemble_tx: graphql.costs.assemble_tx,
                    dry_run: graphql.costs.dry_run,
                    storage_read_replay: graphql.costs.storage_read_replay,
                    submit: graphql.costs.submit,
                    submit_and_await: graphql.costs.submit_and_await,
                    status_change: graphql.costs.status_change,
                    storage_read: graphql.costs.storage_read,
                    tx_get: graphql.costs.tx_get,
                    tx_status_read: graphql.costs.tx_status_read,
                    tx_raw_payload: graphql.costs.tx_raw_payload,
                    block_header: graphql.costs.block_header,
                    block_transactions: graphql.costs.block_transactions,
                    block_transactions_ids: graphql.costs.block_transactions_ids,
                    storage_iterator: graphql.costs.storage_iterator,
                    bytecode_read: graphql.costs.bytecode_read,
                    state_transition_bytecode_read: graphql
                        .costs
                        .state_transition_bytecode_read,
                    da_compressed_block_read: graphql.costs.da_compressed_block_read,
                },
                required_fuel_block_height_tolerance: graphql
                    .required_fuel_block_height_tolerance,
                required_fuel_block_height_timeout: graphql
                    .required_fuel_block_height_timeout
                    .into(),
            },
            combined_db_config,
            snapshot_reader,
            debug,
            historical_execution,
            native_executor_version,
            continue_on_error,
            utxo_validation,
            #[cfg(feature = "parallel-executor")]
            executor_number_of_cores,
            block_production: trigger,
            predefined_blocks_path,
            txpool: TxPoolConfig {
                max_txs_chain_count: tx_max_chain_count,
                max_txs_ttl: tx_pool_ttl,
                ttl_check_interval: tx_ttl_check_interval.into(),
                utxo_validation,
                black_list,
                pool_limits,
                heavy_work: pool_heavy_work_config,
                service_channel_limits,
                pending_pool_tx_ttl: tx_pending_pool_ttl.into(),
                max_pending_pool_size_percentage: tx_pending_pool_size_percentage,
                metrics: metrics.is_enabled(Module::TxPool),
            },
            block_producer: ProducerConfig {
                coinbase_recipient,
                metrics: metrics.is_enabled(Module::Producer),
            },
            gas_price_config,
            block_importer,
            da_compression,
            #[cfg(feature = "relayer")]
            relayer: relayer_cfg,
            #[cfg(feature = "p2p")]
            p2p: p2p_cfg,
            #[cfg(feature = "p2p")]
            sync: sync_args.into(),
            #[cfg(feature = "p2p")]
            pre_confirmation_signature_service: preconfirmation_signature_service_config,
            #[cfg(feature = "shared-sequencer")]
            shared_sequencer: shared_sequencer_args.try_into()?,
            consensus_signer,
            name,
            relayer_consensus_config: verifier,
            min_connected_reserved_peers,
            time_until_synced: time_until_synced.into(),
            production_timeout: production_timeout.into(),
            memory_pool_size,
            tx_status_manager: TxStatusManagerConfig {
                max_tx_update_subscriptions: tx_number_active_subscriptions,
                subscription_ttl,
                status_cache_ttl: status_cache_ttl.into(),
                metrics: metrics.is_enabled(Module::TxStatusManager),
            },
        };
        Ok(config)
    }
}

#[cfg(feature = "rocksdb")]
fn get_default_max_fds() -> i32 {
    getrlimit(Resource::NOFILE)
        .map(|(_, hard)| i32::try_from(hard.saturating_div(2)).unwrap_or(i32::MAX))
        .expect("Our supported platforms should return max FD.")
}

pub async fn get_service_with_shutdown_listeners(
    command: Command,
) -> anyhow::Result<(FuelService, ShutdownListener)> {
    #[cfg(feature = "rocksdb")]
    if command.db_prune && command.database_path.exists() {
        fuel_core::combined_database::CombinedDatabase::prune(&command.database_path)?;
    }

    let profiling = command.profiling.clone();
    let config = command.get_config().await?;

    // start profiling agent if url is configured
    let _profiling_agent = start_pyroscope_agent(profiling, &config)?;

    // log fuel-core version
    info!("Fuel Core version v{}", env!("CARGO_PKG_VERSION"));
    trace!("Initializing in TRACE mode.");
    // initialize the server
    let combined_database = CombinedDatabase::from_config(&config.combined_db_config)?;

    let mut shutdown_listener = ShutdownListener::spawn();

    Ok((
        FuelService::new(combined_database, config, &mut shutdown_listener)?,
        shutdown_listener,
    ))
}

pub async fn get_service(command: Command) -> anyhow::Result<FuelService> {
    let (service, _) = get_service_with_shutdown_listeners(command).await?;
    Ok(service)
}

pub async fn exec(command: Command) -> anyhow::Result<()> {
    let (service, shutdown_listener) =
        get_service_with_shutdown_listeners(command).await?;

    // Genesis could take a long time depending on the snapshot size. Start needs to be
    // interruptible by the shutdown_signal
    tokio::select! {
        result = service.start_and_await() => {
            result?;
        }
        _ = shutdown_listener.wait_until_cancelled() => {
            service.send_stop_signal();
        }
    }

    // pause the main task while service is running
    tokio::select! {
        result = service.await_shutdown() => {
            result?;
        }
        _ = shutdown_listener.wait_until_cancelled() => {}
    }

    service.send_stop_signal_and_await_shutdown().await?;

    Ok(())
}

fn start_pyroscope_agent(
    profiling_args: profiling::ProfilingArgs,
    config: &Config,
) -> anyhow::Result<Option<PyroscopeAgent<PyroscopeAgentRunning>>> {
    profiling_args
        .pyroscope_url
        .as_ref()
        .map(|url| -> anyhow::Result<_> {
            let chain_config = config.snapshot_reader.chain_config();
            let agent = PyroscopeAgent::builder(url, &"fuel-core".to_string())
                .tags(vec![
                    ("service", config.name.as_str()),
                    ("network", chain_config.chain_name.as_str()),
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

#[cfg(test)]
#[allow(non_snake_case)]
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    fn parse_command(args: &[&str]) -> anyhow::Result<Command> {
        Ok(Command::try_parse_from([""].iter().chain(args))?)
    }

    #[test]
    fn parse_disabled_metrics__no_value_enables_everything() {
        // Given
        let args = [];

        // When
        let command = parse_command(&args).unwrap();

        // Then
        let config = command.metrics;
        Module::iter().for_each(|module| {
            assert_eq!(config.is_enabled(module), true);
        });
    }

    #[test]
    fn parse_disabled_metrics__all() {
        // Given
        let args = ["--disable-metrics", "all"];

        // When
        let command = parse_command(&args).unwrap();

        // Then
        let config = command.metrics;
        Module::iter().for_each(|module| {
            assert_eq!(config.is_enabled(module), false);
        });
    }

    #[test]
    fn parse_disabled_metrics__mixed_args() {
        // Given
        let args = [
            "--disable-metrics",
            "txpool,importer",
            "--disable-metrics",
            "graphql",
        ];

        // When
        let command = parse_command(&args).unwrap();

        // Then
        let config = command.metrics;
        assert_eq!(config.is_enabled(Module::TxPool), false);
        assert_eq!(config.is_enabled(Module::Importer), false);
        assert_eq!(config.is_enabled(Module::GraphQL), false);
        assert_eq!(config.is_enabled(Module::P2P), true);
        assert_eq!(config.is_enabled(Module::Producer), true);
    }

    #[test]
    fn parse_disabled_metrics__bad_values() {
        // Given
        let args = ["--disable-metrics", "txpool,alpha,bravo"];

        // When
        let command = parse_command(&args);

        // Then
        let err = command.expect_err("should fail to parse");
        assert_eq!(
            err.to_string(),
            "error: invalid value 'alpha' for \
            '--disable-metrics <METRICS>': Matching variant not found\
            \n\nFor more information, try '--help'.\n"
        );
    }
}
