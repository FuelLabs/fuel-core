use clap::ValueEnum;
use std::{
    num::NonZeroU64,
    path::PathBuf,
    time::Duration,
};
use strum_macros::{
    Display,
    EnumString,
    EnumVariantNames,
};

use fuel_core_chain_config::SnapshotReader;
#[cfg(feature = "test-helpers")]
use fuel_core_chain_config::{
    ChainConfig,
    StateConfig,
};
pub use fuel_core_consensus_module::RelayerConsensusConfig;
pub use fuel_core_importer;
#[cfg(feature = "p2p")]
use fuel_core_p2p::config::{
    Config as P2PConfig,
    NotInitialized,
};
pub use fuel_core_poa::Trigger;
#[cfg(feature = "relayer")]
use fuel_core_relayer::Config as RelayerConfig;
use fuel_core_txpool::config::Config as TxPoolConfig;
use fuel_core_types::{
    blockchain::header::StateTransitionBytecodeVersion,
    signer::SignMode,
};

use crate::{
    combined_database::CombinedDatabaseConfig,
    graphql_api::{
        worker_service::DaCompressionConfig,
        ServiceConfig as GraphQLConfig,
    },
};

#[derive(Clone, Debug)]
pub struct Config {
    pub graphql_config: GraphQLConfig,
    pub combined_db_config: CombinedDatabaseConfig,
    pub snapshot_reader: SnapshotReader,
    pub continue_on_error: bool,
    /// When `true`:
    /// - Enables manual block production.
    /// - Enables debugger endpoint.
    /// - Allows setting `utxo_validation` to `false`.
    pub debug: bool,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    pub native_executor_version: Option<StateTransitionBytecodeVersion>,
    pub block_production: Trigger,
    pub predefined_blocks_path: Option<PathBuf>,
    pub vm: VMConfig,
    pub txpool: TxPoolConfig,
    pub block_producer: fuel_core_producer::Config,
    pub starting_exec_gas_price: u64,
    pub exec_gas_price_change_percent: u16,
    pub min_exec_gas_price: u64,
    pub exec_gas_price_threshold_percent: u8,
    pub da_committer_url: Option<String>,
    pub da_poll_interval: Option<u32>,
    pub da_compression: DaCompressionConfig,
    pub block_importer: fuel_core_importer::Config,
    #[cfg(feature = "relayer")]
    pub relayer: Option<RelayerConfig>,
    #[cfg(feature = "p2p")]
    pub p2p: Option<P2PConfig<NotInitialized>>,
    #[cfg(feature = "p2p")]
    pub sync: fuel_core_sync::Config,
    #[cfg(feature = "shared-sequencer")]
    pub shared_sequencer: fuel_core_shared_sequencer::Config,
    pub consensus_signer: SignMode,
    pub name: String,
    pub relayer_consensus_config: fuel_core_consensus_module::RelayerConsensusConfig,
    /// The number of reserved peers to connect to before starting to sync.
    pub min_connected_reserved_peers: usize,
    /// Time to wait after receiving the latest block before considered to be Synced.
    pub time_until_synced: Duration,
    /// The size of the memory pool in number of `MemoryInstance`s.
    pub memory_pool_size: usize,
    pub da_gas_price_factor: NonZeroU64,
    pub min_da_gas_price: u64,
    pub max_da_gas_price_change_percent: u16,
    pub da_p_component: i64,
    pub da_d_component: i64,
    pub activity_normal_range_size: u16,
    pub activity_capped_range_size: u16,
    pub activity_decrease_range_size: u16,
    pub block_activity_threshold: u8,
}

impl Config {
    #[cfg(feature = "test-helpers")]
    pub fn local_node() -> Self {
        Self::local_node_with_state_config(StateConfig::local_testnet())
    }

    #[cfg(feature = "test-helpers")]
    pub fn local_node_with_state_config(state_config: StateConfig) -> Self {
        Self::local_node_with_configs(ChainConfig::local_testnet(), state_config)
    }

    #[cfg(feature = "test-helpers")]
    pub fn local_node_with_configs(
        chain_config: ChainConfig,
        state_config: StateConfig,
    ) -> Self {
        Self::local_node_with_reader(
            SnapshotReader::new_in_memory(chain_config, state_config),
            None,
        )
    }

    #[cfg(feature = "test-helpers")]
    pub fn local_node_with_reader(
        snapshot_reader: SnapshotReader,
        da_committer_url: Option<String>,
    ) -> Self {
        use crate::state::rocks_db::DatabaseConfig;
        let block_importer = fuel_core_importer::Config::new(false);
        let latest_block = snapshot_reader.last_block_config();
        // In tests, we always want to use the native executor as a default configuration.
        let native_executor_version = latest_block
            .map(|last_block| last_block.state_transition_version.saturating_add(1))
            .unwrap_or(
                fuel_core_types::blockchain::header::LATEST_STATE_TRANSITION_VERSION,
            );

        let utxo_validation = false;

        let combined_db_config = CombinedDatabaseConfig {
            // Set the cache for tests = 10MB
            #[cfg(feature = "rocksdb")]
            database_config: DatabaseConfig {
                cache_capacity: Some(10 * 1024 * 1024),
                columns_policy: Default::default(),
                max_fds: 512,
            },
            database_path: Default::default(),
            #[cfg(feature = "rocksdb")]
            database_type: DbType::RocksDb,
            #[cfg(not(feature = "rocksdb"))]
            database_type: DbType::InMemory,
            #[cfg(feature = "rocksdb")]
            state_rewind_policy:
                crate::state::historical_rocksdb::StateRewindPolicy::RewindFullRange,
        };
        let starting_gas_price = 0;
        let gas_price_change_percent = 0;
        let min_gas_price = 0;
        let gas_price_threshold_percent = 50;

        Self {
            graphql_config: GraphQLConfig {
                addr: std::net::SocketAddr::new(
                    std::net::Ipv4Addr::new(127, 0, 0, 1).into(),
                    0,
                ),
                number_of_threads: 0,
                database_batch_size: 100,
                max_queries_depth: 16,
                max_queries_complexity: 80000,
                max_queries_recursive_depth: 16,
                max_queries_resolver_recursive_depth: 1,
                max_queries_directives: 10,
                max_concurrent_queries: 1024,
                request_body_bytes_limit: 16 * 1024 * 1024,
                query_log_threshold_time: Duration::from_secs(2),
                api_request_timeout: Duration::from_secs(60),
                costs: Default::default(),
            },
            combined_db_config,
            continue_on_error: false,
            debug: true,
            utxo_validation,
            native_executor_version: Some(native_executor_version),
            snapshot_reader,
            block_production: Trigger::Instant,
            predefined_blocks_path: None,
            vm: Default::default(),
            txpool: TxPoolConfig {
                utxo_validation,
                max_txs_ttl: Duration::from_secs(60 * 100000000),
                ..Default::default()
            },
            block_producer: fuel_core_producer::Config {
                ..Default::default()
            },
            da_compression: DaCompressionConfig::Disabled,
            starting_exec_gas_price: starting_gas_price,
            exec_gas_price_change_percent: gas_price_change_percent,
            min_exec_gas_price: min_gas_price,
            exec_gas_price_threshold_percent: gas_price_threshold_percent,
            block_importer,
            #[cfg(feature = "relayer")]
            relayer: None,
            #[cfg(feature = "p2p")]
            p2p: Some(P2PConfig::<NotInitialized>::default("test_network")),
            #[cfg(feature = "p2p")]
            sync: fuel_core_sync::Config::default(),
            #[cfg(feature = "shared-sequencer")]
            shared_sequencer: fuel_core_shared_sequencer::Config::local_node(),
            consensus_signer: SignMode::Key(fuel_core_types::secrecy::Secret::new(
                fuel_core_chain_config::default_consensus_dev_key().into(),
            )),
            name: String::default(),
            relayer_consensus_config: Default::default(),
            min_connected_reserved_peers: 0,
            time_until_synced: Duration::ZERO,
            memory_pool_size: 4,
            da_gas_price_factor: NonZeroU64::new(100).expect("100 is not zero"),
            min_da_gas_price: 0,
            max_da_gas_price_change_percent: 0,
            da_p_component: 0,
            da_d_component: 0,
            activity_normal_range_size: 0,
            activity_capped_range_size: 0,
            activity_decrease_range_size: 0,
            da_committer_url,
            block_activity_threshold: 0,
            da_poll_interval: Some(1_000),
        }
    }

    // TODO: Rework our configs system to avoid nesting of the same configs.
    pub fn make_config_consistent(mut self) -> Config {
        if !self.debug && !self.utxo_validation {
            tracing::warn!(
                "The `utxo_validation` should be `true` with disabled `debug`"
            );
            self.utxo_validation = true;
        }

        if self.txpool.utxo_validation != self.utxo_validation {
            tracing::warn!("The `utxo_validation` of `TxPool` was inconsistent");
            self.txpool.utxo_validation = self.utxo_validation;
        }

        self
    }
}

impl From<&Config> for fuel_core_poa::Config {
    fn from(config: &Config) -> Self {
        fuel_core_poa::Config {
            trigger: config.block_production,
            signer: config.consensus_signer.clone(),
            metrics: false,
            min_connected_reserved_peers: config.min_connected_reserved_peers,
            time_until_synced: config.time_until_synced,
            chain_id: config
                .snapshot_reader
                .chain_config()
                .consensus_parameters
                .chain_id(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VMConfig {
    pub backtrace: bool,
}

#[derive(
    Clone, Copy, Debug, Display, Eq, PartialEq, EnumString, EnumVariantNames, ValueEnum,
)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}
