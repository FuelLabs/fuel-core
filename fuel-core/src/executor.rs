use crate::database::{Database, DatabaseTrait, KvStore, KvStoreError, SharedDatabase};
use crate::model::fuel_block::FuelBlock;
use crate::model::txo::{TransactionOutput, TxoPointer, TxoStatus};
use crate::model::Hash;
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
    pub async fn execute(&self, block: FuelBlock) -> Result<(), Error> {
        let block_tx = self.database.0.transaction();

        for (tx_index, tx_id) in block.transactions.iter().enumerate() {
            let sub_tx = block_tx.transaction();
            let db = sub_tx.as_ref();
            let tx = KvStore::<Bytes32, Transaction>::get(db, &tx_id)?.ok_or(
                Error::MissingTransactionData {
                    block_id: block.id.clone(),
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

    fn _verify_input_state(
        &self,
        transaction: Transaction,
        block: FuelBlock,
    ) -> Result<(), TransactionValidityError> {
        for input in transaction.inputs() {
            match input {
                Input::Coin { utxo_id, .. } => {
                    if let Some(coin) = KvStore::<Bytes32, TransactionOutput>::get(
                        AsRef::<Database>::as_ref(self.database.0.as_ref()),
                        &utxo_id.clone().into(),
                    )? {
                        if coin.status == TxoStatus::Spent {
                            return Err(TransactionValidityError::TxoAlreadySpent);
                        }
                        if block.fuel_height < coin.block_created + coin.maturity {
                            return Err(TransactionValidityError::TxoHasNotMatured);
                        }
                    } else {
                        return Err(TransactionValidityError::TxoDoesntExist);
                    }
                }
                Input::Contract { .. } => {}
            }
        }

        Ok(())
    }

    fn persist_outputs(
        &self,
        block_height: u32,
        tx_index: u32,
        tx: &Transaction,
        db: &Database,
    ) -> Result<(), Error> {
        for (out_idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::Coin { amount, color, to } => Executor::insert_coin(
                    block_height,
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
                    block_height,
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
        let coin = TransactionOutput {
            owner: to.clone(),
            amount: *amount,
            color: *color,
            maturity: 0,
            status: TxoStatus::Unspent,
            block_created: fuel_height,
        };

        if let Some(_) =
            KvStore::<Bytes32, TransactionOutput>::insert(db, &txo_pointer.into(), &coin)?
        {
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
    TxoAlreadySpent,
    #[error("Coin has not yet reached maturity")]
    TxoHasNotMatured,
    #[error("The specified coin doesn't exist")]
    TxoDoesntExist,
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
    #[error("missing transaction data for tx {transaction_id:?} in block {block_id:?}")]
    MissingTransactionData {
        block_id: Hash,
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
