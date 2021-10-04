use crate::database::{KvStore, KvStoreError, SharedDatabase};
use crate::executor::Executor;
use crate::model::fuel_block::FuelBlock;
use chrono::{DateTime, Utc};
use fuel_tx::{Bytes32, Receipt};
use fuel_vm::prelude::{ProgramState, Transaction};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    Submitted {
        time: DateTime<Utc>,
    },
    Success {
        block_id: Bytes32,
        result: ProgramState,
    },
    Failed {
        block_id: Bytes32,
        reason: String,
    },
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("unable to process transaction")]
    InvalidTransaction { reason: String },
    #[error("unexpected database error {0:?}")]
    DatabaseError(Box<dyn StdError>),
    #[error("unexpected block execution error {0:?}")]
    ExecutionError(crate::executor::Error),
}

impl From<KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::DatabaseError(Box::new(e))
    }
}

impl From<crate::state::Error> for Error {
    fn from(e: crate::state::Error) -> Self {
        Error::DatabaseError(Box::new(e))
    }
}

/// Holds submitted transactions and attempts to propose blocks
pub struct TxPool {
    executor: Executor,
    db: SharedDatabase,
}

impl TxPool {
    pub fn new(database: SharedDatabase) -> Self {
        let executor = Executor {
            database: database.clone(),
        };
        TxPool {
            executor,
            db: database,
        }
    }

    pub async fn submit_tx(&self, tx: Transaction) -> Result<Bytes32, Error> {
        let tx_id = tx.id();
        // persist transaction to database
        KvStore::<Bytes32, Transaction>::insert(self.db.as_ref(), &tx_id, &tx)?;
        // set status to submitted
        self.db
            .as_ref()
            .update_tx_status(&tx_id, TransactionStatus::Submitted { time: Utc::now() })?;

        // setup and execute block
        let block_height = self
            .db
            .as_ref()
            .get_block_height()?
            .unwrap_or(Default::default())
            + 1u32.into();
        let block = FuelBlock {
            fuel_height: block_height,
            transactions: vec![tx_id.clone()],
            time: Utc::now(),
            producer: Default::default(),
        };
        // immediately execute block
        self.executor
            .execute(&block)
            .await
            .map_err(|e| Error::ExecutionError(e))?;
        Ok(tx_id)
    }

    pub async fn run_tx(&self, tx: Transaction) -> Result<Vec<Receipt>, Error> {
        let id = self.submit_tx(tx).await?;
        // note: we'll need to await tx completion once it's not instantaneous
        let receipts =
            KvStore::<Bytes32, Vec<Receipt>>::get(self.db.as_ref(), &id)?.unwrap_or_default();
        Ok(receipts)
    }
}
