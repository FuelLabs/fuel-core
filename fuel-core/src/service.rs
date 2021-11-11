use crate::chain_conf::ChainConfig;
use crate::database::Database;
use crate::model::coin::{Coin, CoinStatus, UtxoId};
use crate::tx_pool::TxPool;
use fuel_storage::{MerkleStorage, Storage};
use fuel_types::{Bytes32, ContractId, Salt};
use fuel_vm::prelude::Contract;
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
    /// Create a fuel node instance from service config
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
        // initialize state
        Self::import_state(&config.chain_conf, &database)?;
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

    /// Loads state from the chain config into database
    fn import_state(config: &ChainConfig, database: &Database) -> Result<(), std::io::Error> {
        // start a db transaction for bulk-writing
        let mut import_tx = database.transaction();
        let database = import_tx.as_mut();

        // check if chain is initialized
        if database.get_chain_name()?.is_none() {
            // initialize the chain id
            database.init_chain_name(config.chain_name.clone())?;
            // initialize starting block height if set
            if let Some(height) = config.initial_state.height {
                database.init_chain_height(height)?;
            }
            // initialize coins
            let mut generated_coin_idx = 0;
            for coin in &config.initial_state.coins {
                let utxo_id = coin.utxo_id.unwrap_or_else(|| {
                    generated_coin_idx += 1;
                    UtxoId {
                        tx_id: Default::default(),
                        output_index: generated_coin_idx,
                    }
                    .into()
                });
                let coin = Coin {
                    owner: coin.owner,
                    amount: coin.amount,
                    color: coin.color,
                    maturity: coin.maturity.unwrap_or_default(),
                    status: CoinStatus::Unspent,
                    block_created: coin.block_created.unwrap_or_default(),
                };

                let _ = Storage::<Bytes32, Coin>::insert(database, &utxo_id, &coin)?;
            }

            // initialize contract state
            for contract_config in &config.initial_state.contracts {
                let contract = Contract::from(contract_config.code.as_slice());
                let salt = contract_config.salt;
                let root = contract.root();
                let contract_id = contract.id(&salt, &root);
                // insert contract
                let _ = Storage::<ContractId, Contract>::insert(database, &contract_id, &contract)?;
                // insert contract root
                let _ = Storage::<ContractId, (Salt, Bytes32)>::insert(
                    database,
                    &contract_id,
                    &(salt, root),
                )?;

                // insert state related to contract
                if let Some(contract_state) = &contract_config.state {
                    for (key, value) in contract_state {
                        MerkleStorage::<ContractId, Bytes32, Bytes32>::insert(
                            database,
                            &contract_id,
                            key,
                            value,
                        )?;
                    }
                }
            }
        }

        // Write transaction to db
        import_tx.commit()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_conf::{CoinConfig, StateConfig};
    use crate::model::fuel_block::BlockHeight;

    #[tokio::test]
    async fn config_initializes_chain_name() {
        let test_name = "test_net_123".to_string();
        let service_config = Config {
            chain_conf: ChainConfig {
                chain_name: test_name.clone(),
                ..ChainConfig::local_testnet()
            },
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            test_name,
            db.get_chain_name()
                .unwrap()
                .expect("Expected a chain name to be set")
        )
    }

    #[tokio::test]
    async fn config_initializes_block_height() {
        let test_height = BlockHeight::from(99u32);
        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: StateConfig {
                    coins: vec![],
                    contracts: vec![],
                    height: Some(test_height),
                },
                ..ChainConfig::local_testnet()
            },
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            test_height,
            db.get_block_height()
                .unwrap()
                .expect("Expected a block height to be set")
        )
    }

    #[tokio::test]
    async fn config_state_initializes_coins() {
        let service_config = Config {
            chain_conf: ChainConfig {
                initial_state: StateConfig {
                    coins: vec![CoinConfig {
                        utxo_id: None,
                        block_created: None,
                        maturity: None,
                        owner: Default::default(),
                        amount: 45,
                        color: Default::default(),
                    }],
                    contracts: vec![],
                    height: None,
                },
                ..ChainConfig::local_testnet()
            },
            ..Config::local_node()
        };

        let db = Database::default();
        FuelService::from_database(db.clone(), service_config)
            .await
            .unwrap();

        assert_eq!(
            test_height,
            db.get_block_height()
                .unwrap()
                .expect("Expected a block height to be set")
        )
    }
}
