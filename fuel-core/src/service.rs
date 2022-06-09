use crate::{chain_config::ChainConfig, database::Database};
use anyhow::Error as AnyError;
use modules::Modules;
use std::{
    net::{Ipv4Addr, SocketAddr},
    panic,
    path::PathBuf,
};
use strum_macros::{Display, EnumString, EnumVariantNames};
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::log::warn;

pub(crate) mod genesis;
pub mod graph_api;
pub mod modules;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: SocketAddr,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
    // default to false until downstream consumers stabilize
    pub utxo_validation: bool,
    // default to false until predicates have fully stabilized
    pub predicates: bool,
    pub vm: VMConfig,
    pub tx_pool_config: fuel_txpool::Config,
}

impl Config {
    pub fn local_node() -> Self {
        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            database_path: Default::default(),
            database_type: DbType::InMemory,
            chain_conf: ChainConfig::local_testnet(),
            vm: Default::default(),
            utxo_validation: false,
            predicates: false,
            tx_pool_config: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VMConfig {
    pub backtrace: bool,
}

#[derive(Clone, Debug, Display, PartialEq, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
pub enum DbType {
    InMemory,
    RocksDb,
}

pub struct FuelService {
    tasks: Vec<JoinHandle<Result<(), AnyError>>>,
    /// handler for all modules.
    modules: Modules,
    /// The address bound by the system for serving the API
    pub bound_address: SocketAddr,
}

impl FuelService {
    /// Create a fuel node instance from service config
    #[tracing::instrument(skip(config))]
    pub async fn new_node(config: Config) -> Result<Self, AnyError> {
        // initialize database
        let database = match config.database_type {
            #[cfg(feature = "rocksdb")]
            DbType::RocksDb => Database::open(&config.database_path)?,
            DbType::InMemory => Database::in_memory(),
            #[cfg(not(feature = "rocksdb"))]
            _ => Database::in_memory(),
        };
        // initialize service
        Self::init_service(database, config).await
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Used to initialize a service with a pre-existing database
    pub async fn from_database(database: Database, config: Config) -> Result<Self, AnyError> {
        Self::init_service(database, config).await
    }

    /// Private inner method for initializing the fuel service
    async fn init_service(database: Database, config: Config) -> Result<Self, AnyError> {
        // check predicates flag
        if config.predicates {
            warn!("Predicates are currently an unstable feature!");
        }

        // initialize state
        Self::import_state(&config.chain_conf, &database)?;

        // start modules
        let modules = modules::start_modules(&config, &database).await?;

        // start background tasks
        let mut tasks = vec![];
        let (bound_address, api_server) =
            graph_api::start_server(config, database, &modules).await?;
        tasks.push(api_server);

        Ok(FuelService {
            tasks,
            bound_address,
            modules,
        })
    }

    /// Awaits for the completion of any server background tasks
    pub async fn run(self) {
        for task in self.tasks {
            match task.await {
                Err(err) => {
                    if err.is_panic() {
                        // Resume the panic on the main task
                        panic::resume_unwind(err.into_panic());
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("server error: {:?}", e);
                }
                Ok(Ok(_)) => {}
            }
        }
    }

    /// Shutdown background tasks
    pub async fn stop(&self) {
        for task in &self.tasks {
            task.abort();
        }
        self.modules.stop().await;
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("An api server error occurred {0}")]
    ApiServer(#[from] hyper::Error),
}
