use fuel_core_storage::{
    Error as StorageError,
    IsNotFound,
};
use std::{
    net::SocketAddr,
    time::Duration,
};

pub mod api_service;
pub mod database;
pub(crate) mod metrics_extension;
pub mod ports;
pub mod storage;
pub(crate) mod view_extension;
pub mod worker_service;

#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub addr: SocketAddr,
    pub max_queries_depth: usize,
    pub max_queries_complexity: usize,
    pub max_queries_recursive_depth: usize,
    pub request_body_bytes_limit: usize,
    /// Time to wait after submitting a query before debug info will be logged about query.
    pub query_log_threshold_time: Duration,
    pub api_request_timeout: Duration,
}

pub struct Costs {
    pub balance_query: usize,
    pub coins_to_spend: usize,
    pub get_peers: usize,
    pub estimate_predicates: usize,
    pub dry_run: usize,
    pub submit: usize,
    pub submit_and_await: usize,
    pub status_change: usize,
    pub raw_payload: usize,
    pub storage_read: usize,
    pub storage_iterator: usize,
    pub bytecode_read: usize,
}

pub const QUERY_COSTS: Costs = Costs {
    // balance_query: 4000,
    balance_query: 10001,
    coins_to_spend: 10001,
    // get_peers: 2000,
    get_peers: 10001,
    // estimate_predicates: 3000,
    estimate_predicates: 10001,
    dry_run: 3000,
    // submit: 5000,
    submit: 10001,
    submit_and_await: 10001,
    status_change: 10001,
    raw_payload: 10,
    storage_read: 10,
    storage_iterator: 100,
    bytecode_read: 2000,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub config: ServiceConfig,
    pub utxo_validation: bool,
    pub debug: bool,
    pub vm_backtrace: bool,
    pub max_tx: usize,
    pub max_txpool_depth: usize,
    pub chain_name: String,
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
