use crate::database::{KvStore, KvStoreError, SharedDatabase};
use crate::executor::Executor;
use crate::model::fuel_block::FuelBlock;
use crate::model::Hash;
use fuel_tx::Bytes32;
use fuel_vm::prelude::Transaction;
use std::convert::TryInto;
use std::error::Error as StdError;
use thiserror::Error;

pub enum TransactionStatus {
    Submitted,
    InBlock { block_id: Hash },
    Failed { reason: String },
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("unable to process transaction")]
    InvalidTransaction { reason: String },
    #[error("unexpected database error {0:?}")]
    DatabaseError(Box<dyn StdError>),
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
        KvStore::<Bytes32, Transaction>::insert(self.db.0.as_ref().as_ref(), &tx_id, &tx)?;

        let mut tx_updates: Vec<TransactionStatus> = vec![];
        tx_updates.push(TransactionStatus::Submitted);

        // setup and execute block
        let block_height = self
            .db
            .as_ref()
            .get_block_height()?
            .unwrap_or(Default::default())
            + 1;
        let mut block_id = [0u8; 28].to_vec();
        block_id.extend_from_slice(&block_height.to_be_bytes());
        let block_id: Hash = block_id
            .try_into()
            .expect("block id is constructed to the exact length");

        let evt = self
            .executor
            .execute(FuelBlock {
                id: block_id.clone(),
                fuel_height: block_height,
                transactions: vec![tx_id.clone()],
            })
            .await
            .map_or_else(
                |e| TransactionStatus::Failed {
                    reason: format!("{:?}", e),
                },
                |_| TransactionStatus::InBlock { block_id },
            );
        tx_updates.push(evt);
        // TODO: push tx updates to channel for subscribers
        Ok(tx_id)
    }
}
