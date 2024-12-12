use fuel_core_storage::{
    Error as StorageError,
    IsNotFound,
};
use std::{
    net::SocketAddr,
    sync::OnceLock,
    time::Duration,
};

pub mod api_service;
pub mod da_compression;
pub mod database;
pub(crate) mod indexation;
pub(crate) mod metrics_extension;
pub mod ports;
pub mod storage;
pub(crate) mod validation_extension;
pub(crate) mod view_extension;
pub mod worker_service;

#[derive(Clone, Debug)]
pub struct Config {
    pub config: ServiceConfig,
    pub utxo_validation: bool,
    pub debug: bool,
    pub vm_backtrace: bool,
    pub max_tx: usize,
    pub max_txpool_dependency_chain_length: usize,
    pub chain_name: String,
}

#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub addr: SocketAddr,
    pub number_of_threads: usize,
    pub database_batch_size: usize,
    pub max_queries_depth: usize,
    pub max_queries_complexity: usize,
    pub max_queries_recursive_depth: usize,
    pub max_queries_resolver_recursive_depth: usize,
    pub max_queries_directives: usize,
    pub max_concurrent_queries: usize,
    pub request_body_bytes_limit: usize,
    /// Time to wait after submitting a query before debug info will be logged about query.
    pub query_log_threshold_time: Duration,
    pub api_request_timeout: Duration,
    /// Configurable cost parameters to limit graphql queries complexity
    pub costs: Costs,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Costs {
    pub balance_query: usize,
    pub coins_to_spend: usize,
    pub get_peers: usize,
    pub estimate_predicates: usize,
    pub dry_run: usize,
    pub execution_trace_block: usize,
    pub submit: usize,
    pub submit_and_await: usize,
    pub status_change: usize,
    pub storage_read: usize,
    pub tx_get: usize,
    pub tx_status_read: usize,
    pub tx_raw_payload: usize,
    pub block_header: usize,
    pub block_transactions: usize,
    pub block_transactions_ids: usize,
    pub storage_iterator: usize,
    pub bytecode_read: usize,
    pub state_transition_bytecode_read: usize,
    pub da_compressed_block_read: usize,
}

#[cfg(feature = "test-helpers")]
impl Default for Costs {
    fn default() -> Self {
        DEFAULT_QUERY_COSTS
    }
}

pub const DEFAULT_QUERY_COSTS: Costs = Costs {
    // TODO: The cost of the `balance` and `balances` query should depend on the
    //  `OffChainDatabase::balances_enabled` value. If additional indexation is enabled,
    //  the cost should be cheaper.
    balance_query: 40001,
    coins_to_spend: 40001,
    get_peers: 40001,
    estimate_predicates: 40001,
    dry_run: 12000,
    execution_trace_block: 1_000_000,
    submit: 40001,
    submit_and_await: 40001,
    status_change: 40001,
    storage_read: 40,
    tx_get: 50,
    tx_status_read: 50,
    tx_raw_payload: 150,
    block_header: 150,
    block_transactions: 1500,
    block_transactions_ids: 50,
    storage_iterator: 100,
    bytecode_read: 8000,
    state_transition_bytecode_read: 76_000,
    da_compressed_block_read: 4000,
};

pub fn query_costs() -> &'static Costs {
    QUERY_COSTS.get().unwrap_or(&DEFAULT_QUERY_COSTS)
}

pub static QUERY_COSTS: OnceLock<Costs> = OnceLock::new();

fn initialize_query_costs(costs: Costs) -> anyhow::Result<()> {
    #[cfg(feature = "test-helpers")]
    if costs != DEFAULT_QUERY_COSTS {
        // We don't support setting these values in test contexts, because
        // it can lead to unexpected behavior if multiple tests try to
        // initialize different values.
        anyhow::bail!("cannot initialize queries with non-default costs in tests")
    }

    QUERY_COSTS.get_or_init(|| costs);

    Ok(())
}

pub trait IntoApiResult<T> {
    fn into_api_result<NewT, E>(self) -> Result<Option<NewT>, E>
    where
        NewT: From<T>,
        E: From<StorageError>;
}

impl<T> IntoApiResult<T> for Result<T, StorageError> {
    fn into_api_result<NewT, E>(self) -> Result<Option<NewT>, E>
    where
        NewT: From<T>,
        E: From<StorageError>,
    {
        if self.is_not_found() {
            Ok(None)
        } else {
            Ok(Some(self?.into()))
        }
    }
}
