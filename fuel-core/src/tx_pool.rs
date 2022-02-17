use crate::database::{Database, KvStoreError};
use crate::executor::Executor;
use crate::model::fuel_block::FuelBlock;
use crate::service::Config;
use chrono::{DateTime, Utc};
use fuel_core_interfaces::txpool::{TxPool as TxPoolTrait, TxPoolDb};
use fuel_storage::Storage;
use fuel_tx::{Bytes32, Receipt};
use fuel_txpool::{Config as TxPoolConfig, TxPoolService};
use fuel_vm::prelude::{ProgramState, Transaction};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::sync::Arc;
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    Submitted {
        time: DateTime<Utc>,
    },
    Success {
        block_id: Bytes32,
        time: DateTime<Utc>,
        result: ProgramState,
    },
    Failed {
        block_id: Bytes32,
        time: DateTime<Utc>,
        reason: String,
        result: Option<ProgramState>,
    },
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("unexpected database error {0:?}")]
    Database(Box<dyn StdError>),
    #[error("unexpected block execution error {0:?}")]
    Execution(crate::executor::Error),
}

impl From<KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::Database(Box::new(e))
    }
}

impl From<crate::state::Error> for Error {
    fn from(e: crate::state::Error) -> Self {
        Error::Database(Box::new(e))
    }
}

impl TxPoolDb for Database {}

/// Holds submitted transactions and attempts to propose blocks
pub struct TxPool {
    executor: Executor,
    db: Database,
    fuel_txpool: Box<dyn TxPoolTrait>,
}

impl TxPool {
    pub fn pool(&self) -> &dyn TxPoolTrait {
        self.fuel_txpool.as_ref()
    }

    pub fn new(database: Database) -> Self {
        let executor = Executor {
            database: database.clone(),
        };
        let config = Arc::new(TxPoolConfig::default());
        TxPool {
            executor,
            db: database.clone(),
            fuel_txpool: Box::new(TxPoolService::new(
                Box::new(database) as Box<dyn TxPoolDb>,
                config,
            )),
        }
    }

    pub async fn submit_tx(&self, tx: Transaction, config: &Config) -> Result<Bytes32, Error> {
        let tx_id = tx.id();
        // persist transaction to database
        let mut db = self.db.clone();
        Storage::<Bytes32, Transaction>::insert(&mut db, &tx_id, &tx)?;
        // set status to submitted
        db.update_tx_status(&tx_id, TransactionStatus::Submitted { time: Utc::now() })?;

        // setup and execute block
        let block_height = db.get_block_height()?.unwrap_or_default() + 1u32.into();
        let block = FuelBlock {
            fuel_height: block_height,
            transactions: vec![tx_id],
            time: Utc::now(),
            producer: Default::default(),
        };
        // immediately execute block
        self.executor
            .execute(&block, &config.vm)
            .await
            .map_err(Error::Execution)?;
        Ok(tx_id)
    }

    pub async fn run_tx(&self, tx: Transaction, config: &Config) -> Result<Vec<Receipt>, Error> {
        let id = self.submit_tx(tx, config).await?;
        // note: we'll need to await tx completion once it's not instantaneous
        let db = &self.db;
        let receipts = Storage::<Bytes32, Vec<Receipt>>::get(db, &id)?.unwrap_or_default();
        Ok(receipts.into_owned())
    }
}
