use crate::database::{Database, KvStore, KvStoreError, SharedDatabase};
use crate::model::coin::{Coin, CoinId, CoinStatus};
use crate::model::fuel_block::FuelBlock;
use fuel_tx::{Input, Transaction};
use std::error::Error as StdError;
use thiserror::Error;

pub struct Executor {
    database: SharedDatabase,
}

impl Executor {
    pub async fn execute(&self, block: FuelBlock) -> Result<(), Error> {
        Ok(())
    }

    fn verify_input_state(
        &self,
        transaction: Transaction,
        block: FuelBlock,
    ) -> Result<(), TransactionValidityError> {
        for input in transaction.inputs() {
            match input {
                Input::Coin { utxo_id, .. } => {
                    if let Some(coin) = KvStore::<CoinId, Coin>::get(
                        AsRef::<Database>::as_ref(self.database.0.as_ref()),
                        &utxo_id.clone().into(),
                    )? {
                        if coin.status == CoinStatus::Spent {
                            return Err(TransactionValidityError::CoinAlreadySpent);
                        }
                        if block.fuel_height < coin.block_created + coin.maturity {
                            return Err(TransactionValidityError::CoinHasNotMatured);
                        }
                    } else {
                        return Err(TransactionValidityError::CoinDoesntExist);
                    }
                }
                Input::Contract { .. } => {}
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum TransactionValidityError {
    #[error("Coin input was already spent")]
    CoinAlreadySpent,
    #[error("Coin has not yet reached maturity")]
    CoinHasNotMatured,
    #[error("The specified coin doesn't exist")]
    CoinDoesntExist,
    #[error("Datastore error occurred")]
    DataStoreError(Box<dyn std::error::Error>),
}

impl From<crate::database::KvStoreError> for TransactionValidityError {
    fn from(e: crate::database::KvStoreError) -> Self {
        Self::DataStoreError(Box::new(e))
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("a block production error occurred")]
    BlockExecutionError(Box<dyn StdError>),
}
