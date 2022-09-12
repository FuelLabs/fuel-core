use crate::database::Database;
use anyhow::Error as AnyError;
use modules::Modules;
use std::{
    net::SocketAddr,
    panic,
};
use tokio::task::JoinHandle;
use tracing::log::warn;

pub use config::{
    Config,
    DbType,
    VMConfig,
};

pub mod config;
pub(crate) mod genesis;
pub mod graph_api;
pub mod metrics;
pub mod modules;

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
    pub async fn from_database(
        database: Database,
        config: Config,
    ) -> Result<Self, AnyError> {
        Self::init_service(database, config).await
    }

    /// Private inner method for initializing the fuel service
    async fn init_service(database: Database, config: Config) -> Result<Self, AnyError> {
        // check predicates flag
        if config.predicates {
            warn!("Predicates are currently an unstable feature!");
        }

        // initialize state
        Self::initialize_state(&config, &database)?;

        // start modules
        let modules = modules::start_modules(&config, &database).await?;

        // start background tasks
        let mut tasks = vec![];
        let (bound_address, api_server) =
            graph_api::start_server(config.clone(), database, &modules).await?;
        tasks.push(api_server);
        // Socket is ignored for now, but as more services are added
        // it may be helpful to have a way to list all services and their ports

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
