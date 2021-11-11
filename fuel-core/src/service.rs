use crate::chain_conf::ChainConfig;
use crate::database::Database;
use crate::tx_pool::TxPool;
pub use graph_api::start_server;
#[cfg(feature = "rocksdb")]
use std::io::ErrorKind;
use std::{
    net::{self, Ipv4Addr, SocketAddr},
    panic,
    path::PathBuf,
    sync::Arc,
};
use strum_macros::{Display, EnumString};
use thiserror::Error;
use tokio::task::JoinHandle;

pub mod graph_api;

#[derive(Clone, Debug)]
pub struct Config {
    pub addr: net::SocketAddr,
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub chain_conf: ChainConfig,
}

impl Config {
    pub fn local_node() -> Self {
        Self {
            addr: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 0),
            database_path: Default::default(),
            database_type: DbType::InMemory,
            chain_conf: ChainConfig::local_testnet(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, EnumString, Display)]
pub enum DbType {
    InMemory,
    RocksDb,
}

pub struct FuelService {
    tasks: Vec<JoinHandle<Result<(), Error>>>,
    /// The address bound by the system for serving the API
    pub bound_address: SocketAddr,
}

impl FuelService {
    pub async fn new_node(config: Config) -> Result<Self, std::io::Error> {
        // initialize database
        let database = match config.database_type {
            #[cfg(feature = "rocksdb")]
            DbType::RocksDb => Database::open(&config.database_path)
                .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?,
            DbType::InMemory => Database::default(),
            #[cfg(not(feature = "rocksdb"))]
            _ => Database::default(),
        };
        // initialize state
        Self::import_state(&config.chain_conf, &database)?;
        // initialize service
        Self::init_service(database, config).await
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Used to initialize a service with a pre-existing database
    pub async fn from_database(database: Database, config: Config) -> Result<Self, std::io::Error> {
        Self::init_service(database, config).await
    }

    /// Private inner method for initializing the fuel service
    async fn init_service(database: Database, config: Config) -> Result<Self, std::io::Error> {
        // initialize transaction pool
        let tx_pool = Arc::new(TxPool::new(database.clone()));

        // start background tasks
        let mut tasks = vec![];
        let (bound_address, api_server) =
            graph_api::start_server(config, database, tx_pool).await?;
        tasks.push(api_server);

        Ok(FuelService {
            tasks,
            bound_address,
        })
    }

    fn import_state(config: &ChainConfig, database: &Database) -> Result<(), std::io::Error> {
        Ok(())
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
    pub fn stop(&self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("An api server error occurred {0}")]
    ApiServer(#[from] hyper::Error),
}
