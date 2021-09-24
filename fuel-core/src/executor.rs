use crate::database::{Database, DatabaseTrait, KvStore, KvStoreError, SharedDatabase};
use crate::model::coin::{Coin, CoinStatus, TxoPointer};
use crate::model::fuel_block::{BlockHeight, FuelBlock};
use fuel_asm::Word;
use fuel_tx::{Address, Bytes32, Color, Input, Output, Receipt, Transaction};
use fuel_vm::interpreter::ExecuteError;
use fuel_vm::prelude::Interpreter;
use std::error::Error as StdError;
use thiserror::Error;

pub struct Executor {
    pub(crate) database: SharedDatabase,
}

impl Executor {
    pub async fn execute(&self, block: &FuelBlock) -> Result<(), Error> {
        let block_tx = self.database.0.transaction();
        let block_id = block.id();
        KvStore::<Bytes32, FuelBlock>::insert(block_tx.as_ref(), &block_id, &block)?;

        for (tx_index, tx_id) in block.transactions.iter().enumerate() {
            let sub_tx = block_tx.transaction();
            let db = sub_tx.as_ref();
            let tx = KvStore::<Bytes32, Transaction>::get(db, &tx_id)?.ok_or(
                Error::MissingTransactionData {
                    block_id: block_id.clone(),
                    transaction_id: tx_id.clone(),
                },
            )?;

            // execute vm
            let mut vm = Interpreter::with_storage(db.clone());
            let execution_result = vm.transact(tx);

            if let Ok(executed_tx) = execution_result {
                // persist any outputs
                self.persist_outputs(block.fuel_height, tx_index as u32, executed_tx.tx(), db)?;

                // persist receipts
                self.persist_receipts(tx_id, vm.receipts(), db)?;

                // only commit state changes if execution was a success
                sub_tx.commit()?;
            } else {
                // how do we want to log failed transactions?
            }
        }

        block_tx.commit()?;
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
                    if let Some(coin) = KvStore::<Bytes32, Coin>::get(
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

    fn persist_outputs(
        &self,
        block_height: BlockHeight,
        tx_index: u32,
        tx: &Transaction,
        db: &Database,
    ) -> Result<(), Error> {
        for (out_idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::Coin { amount, color, to } => Executor::insert_coin(
                    block_height.into(),
                    tx_index,
                    out_idx as u8,
                    amount,
                    color,
                    to,
                    db,
                )?,
                Output::Contract {
                    balance_root: _,
                    input_index: _,
                    state_root: _,
                } => {}
                Output::Withdrawal { .. } => {}
                Output::Change { to, color, amount } => Executor::insert_coin(
                    block_height.into(),
                    tx_index,
                    out_idx as u8,
                    amount,
                    color,
                    to,
                    db,
                )?,
                Output::Variable { .. } => {}
                Output::ContractCreated { .. } => {}
            }
        }
        Ok(())
    }

    fn insert_coin(
        fuel_height: u32,
        tx_index: u32,
        out_index: u8,
        amount: &Word,
        color: &Color,
        to: &Address,
        db: &Database,
    ) -> Result<(), Error> {
        let txo_pointer = TxoPointer {
            block_height: fuel_height,
            tx_index,
            output_index: out_index,
        };
        let coin = Coin {
            owner: to.clone(),
            amount: *amount,
            color: *color,
            maturity: 0u32.into(),
            status: CoinStatus::Unspent,
            block_created: fuel_height.into(),
        };

        if let Some(_) = KvStore::<Bytes32, Coin>::insert(db, &txo_pointer.into(), &coin)? {
            return Err(Error::OutputAlreadyExists);
        }
        Ok(())
    }

    pub fn persist_receipts(
        &self,
        tx_id: &Bytes32,
        receipts: &[Receipt],
        db: &Database,
    ) -> Result<(), Error> {
        if let Some(_) = KvStore::<Bytes32, Vec<Receipt>>::insert(db, tx_id, &Vec::from(receipts))?
        {
            return Err(Error::OutputAlreadyExists);
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
    #[error("output already exists")]
    OutputAlreadyExists,
    #[error("corrupted block state")]
    CorruptedBlockState(Box<dyn StdError>),
    #[error("a block production error occurred")]
    BlockExecutionError(Box<dyn StdError>),
    #[error("missing transaction data for tx {transaction_id:?} in block {block_id:?}")]
    MissingTransactionData {
        block_id: Bytes32,
        transaction_id: Bytes32,
    },
    #[error("VM execution error: {0:?}")]
    VmExecutionError(fuel_vm::interpreter::ExecuteError),
}

impl From<crate::database::KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}

impl From<fuel_vm::interpreter::ExecuteError> for Error {
    fn from(e: ExecuteError) -> Self {
        Error::VmExecutionError(e)
    }
}

impl From<crate::state::Error> for Error {
    fn from(e: crate::state::Error) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}
