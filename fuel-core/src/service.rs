use crate::{chain_config::ChainConfig, database::Database, tx_pool::TxPool};
use anyhow::Error as AnyError;
use fuel_block_importer::{Config as FuelBlockImporterConfig, Service as FuelBlockImporterService};
use fuel_block_producer::{Config as FuelBlockProducerConfig, Service as FuelBlockProducerService};
use fuel_core_bft::{Config as FuelCoreBftConfig, Service as FuelCoreBftService};
use fuel_core_interfaces::{model, *};
use fuel_sync::{Config as FuelSyncConfig, Service as FuelSyncService};
use std::{
    net::{Ipv4Addr, SocketAddr},
    panic,
    path::PathBuf,
    sync::Arc,
};
use strum_macros::{Display, EnumString, EnumVariantNames};
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::log::warn;
// TODO chenge naming of txpool service to be similar to others
use fuel_txpool::{Config as FuelTxpoolConfig, TxPoolService as FuelTxpoolService};
// TOOD include for p2p and relayer

pub use graph_api::start_server;

pub(crate) mod genesis;
pub mod graph_api;

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

        let db = ();
        
        // initialize state
        Self::import_state(&config.chain_conf, &database)?;
        // initialize transaction pool
        let tx_pool = Arc::new(TxPool::new(database.clone(), config.clone()));
        // Initialize and bind all components
        let mut block_importer =
            FuelBlockImporterService::new(&FuelBlockImporterConfig::default(), db).await?;
        let mut block_producer =
            FuelBlockProducerService::new(&FuelBlockProducerConfig::default(), db).await?;
        let mut bft = FuelCoreBftService::new(&FuelCoreBftConfig::default(), db).await?;
        let mut sync = FuelSyncService::new(&FuelSyncConfig::default()).await?;
        
        // let mut relayer = FuelRelayer::new(FuelRelayerConfig::default());
        // let mut p2p = FuelP2P::new(FuelP2PConfig::default());
        // let mut txpool = FuelTxpool::new(FuelTxpoolConfig::default());
        let txpool_mpsc = ();
        let p2p_mpsc = ();
        let p2p_broadcast_consensus = ();
        let relayer_mpsc = ();

        block_importer.start().await;
        block_producer.start(txpool_mpsc).await;
        bft.start(
            relayer_mpsc,
            block_producer.sender().clone(),
            block_importer.sender().clone(),
            block_importer.subscribe(),
        )
        .await;
        sync.start().await;

        // start background tasks
        let mut tasks = vec![];
        let (bound_address, api_server) = start_server(config, database, tx_pool).await?;
        tasks.push(api_server);

        Ok(FuelService {
            tasks,
            bound_address,
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
