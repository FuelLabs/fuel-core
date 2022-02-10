use crate::database::transaction::TransactionIndex;
use crate::model::fuel_block::FuelBlockFull;
use crate::service::VMConfig;
use crate::{
    database::{Database, KvStoreError},
    model::{
        coin::{Coin, CoinStatus},
        fuel_block::{BlockHeight, FuelBlockLight},
    },
    tx_pool::TransactionStatus,
};
use fuel_asm::Word;
use fuel_storage::Storage;
use fuel_tx::{Address, Bytes32, Color, Input, Output, Receipt, Transaction, UtxoId};
use fuel_vm::{
    consts::REG_SP,
    prelude::{Backtrace as FuelBacktrace, InterpreterError},
    transactor::Transactor,
};
use std::error::Error as StdError;
use std::ops::DerefMut;
use thiserror::Error;
use tracing::warn;

///! The executor is used for block production and validation. Given a block, it will execute all
/// the transactions contained in the block and persist changes to the underlying database as needed.
/// In production mode, block fields like transaction commitments are set based on the executed txs.
/// In validation mode, the processed block commitments are compared with the proposed block.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionMode {
    Production,
    Validation,
}

pub struct Executor {
    pub database: Database,
    pub config: VMConfig,
}

impl Executor {
    pub async fn execute(
        &self,
        block: &mut FuelBlockFull,
        mode: ExecutionMode,
    ) -> Result<(), Error> {
        let mut block_db_commit = self.database.transaction();
        let block_id = block.id();

        for (idx, tx) in block.transactions.iter_mut().enumerate() {
            let mut sub_block_db_commit = block_db_commit.transaction();
            // A database view that only lives for the duration of the transaction
            let sub_db_view = sub_block_db_commit.deref_mut();
            let tx_id = tx.id();

            // index owners of inputs and outputs with tx-id, regardless of validity (hence block_tx instead of tx_db)
            self.persist_owners_index(
                block.headers.fuel_height,
                &tx,
                &tx_id,
                idx,
                block_db_commit.deref_mut(),
            )?;

            // execute vm
            let mut vm = Transactor::new(sub_db_view.clone());
            vm.transact(tx.clone());
            match vm.result() {
                Ok(result) => {
                    if mode == ExecutionMode::Validation {
                        // ensure tx matches vm output exactly
                        if result.tx() != tx {
                            return Err(Error::InvalidTransactionOutcome);
                        }
                    } else {
                        // malleate the block with the resultant tx from the vm
                        *tx = result.tx().clone()
                    }

                    Storage::<Bytes32, Transaction>::insert(sub_db_view, &tx_id, result.tx())?;

                    // persist any outputs
                    self.persist_outputs(block.headers.fuel_height, result.tx(), sub_db_view)?;

                    // persist receipts
                    self.persist_receipts(&tx_id, result.receipts(), sub_db_view)?;

                    let status = if result.should_revert() {
                        self.log_backtrace(&vm);
                        // if script result exists, log reason
                        if let Some((script_result, _)) = result.receipts().iter().find_map(|r| {
                            if let Receipt::ScriptResult { result, gas_used } = r {
                                Some((result, gas_used))
                            } else {
                                None
                            }
                        }) {
                            TransactionStatus::Failed {
                                block_id,
                                time: block.headers.time,
                                reason: format!("{:?}", script_result.reason()),
                                result: Some(*result.state()),
                            }
                        }
                        // otherwise just log the revert arg
                        else {
                            TransactionStatus::Failed {
                                block_id,
                                time: block.headers.time,
                                reason: format!("{:?}", result.state()),
                                result: Some(*result.state()),
                            }
                        }
                    } else {
                        // else tx was a success
                        TransactionStatus::Success {
                            block_id,
                            time: block.headers.time,
                            result: *result.state(),
                        }
                    };

                    // persist tx status at the block level
                    block_db_commit.update_tx_status(&tx_id, status)?;

                    // only commit state changes if execution was a success
                    if !result.should_revert() {
                        sub_block_db_commit.commit()?;
                    }
                }
                Err(e) => {
                    // TODO: if it's a validation related error, then this block should be marked
                    //       as invalid and the `block_tx` dropped / uncommitted.
                    // save error status on block_tx since the sub_tx changes are dropped
                    block_db_commit.update_tx_status(
                        &tx_id,
                        TransactionStatus::Failed {
                            block_id,
                            time: block.headers.time,
                            reason: e.to_string(),
                            result: None,
                        },
                    )?;
                }
            }
        }

        // insert block into database
        Storage::<Bytes32, FuelBlockLight>::insert(
            block_db_commit.deref_mut(),
            &block_id,
            &block.as_light(),
        )?;
        block_db_commit.commit()?;
        Ok(())
    }

    // Waiting until accounts and genesis block setup is working
    fn _verify_input_state(
        &self,
        transaction: Transaction,
        block: FuelBlockLight,
    ) -> Result<(), TransactionValidityError> {
        let db = &self.database;
        for input in transaction.inputs() {
            match input {
                Input::Coin { utxo_id, .. } => {
                    if let Some(coin) = Storage::<UtxoId, Coin>::get(db, &utxo_id)? {
                        if coin.status == CoinStatus::Spent {
                            return Err(TransactionValidityError::CoinAlreadySpent);
                        }
                        if block.headers.fuel_height < coin.block_created + coin.maturity {
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

    /// Log a VM backtrace if configured to do so
    fn log_backtrace(&self, transactor: &Transactor<'_, Database>) {
        if self.config.backtrace {
            if let Some(backtrace) = transactor.backtrace() {
                warn!(
                    target = "vm",
                    "Backtrace on contract: 0x{:x}\nregisters: {:?}\ncall_stack: {:?}\nstack\n: {}",
                    backtrace.contract(),
                    backtrace.registers(),
                    backtrace.call_stack(),
                    hex::encode(&backtrace.memory()[..backtrace.registers()[REG_SP] as usize]), // print stack
                );
            }
        }
    }

    fn persist_outputs(
        &self,
        block_height: BlockHeight,
        tx: &Transaction,
        db: &mut Database,
    ) -> Result<(), Error> {
        let id = tx.id();
        for (out_idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::Coin { amount, color, to } => Executor::insert_coin(
                    block_height.into(),
                    id,
                    out_idx as u8,
                    &amount,
                    &color,
                    &to,
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
                    id,
                    out_idx as u8,
                    &amount,
                    &color,
                    &to,
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
        tx_id: Bytes32,
        output_index: u8,
        amount: &Word,
        color: &Color,
        to: &Address,
        db: &mut Database,
    ) -> Result<(), Error> {
        let utxo_id = UtxoId::new(tx_id, output_index);
        let coin = Coin {
            owner: *to,
            amount: *amount,
            color: *color,
            maturity: 0u32.into(),
            status: CoinStatus::Unspent,
            block_created: fuel_height.into(),
        };

        if Storage::<UtxoId, Coin>::insert(db, &utxo_id, &coin)?.is_some() {
            return Err(Error::OutputAlreadyExists);
        }
        Ok(())
    }

    fn persist_receipts(
        &self,
        tx_id: &Bytes32,
        receipts: &[Receipt],
        db: &mut Database,
    ) -> Result<(), Error> {
        if Storage::<Bytes32, Vec<Receipt>>::insert(db, tx_id, &Vec::from(receipts))?.is_some() {
            return Err(Error::OutputAlreadyExists);
        }
        Ok(())
    }

    /// Index the tx id by owner for all of the inputs and outputs
    fn persist_owners_index(
        &self,
        block_height: BlockHeight,
        tx: &Transaction,
        tx_id: &Bytes32,
        tx_idx: usize,
        db: &mut Database,
    ) -> Result<(), Error> {
        let mut owners = vec![];
        for input in tx.inputs() {
            if let Input::Coin { owner, .. } = input {
                owners.push(owner);
            }
        }

        for output in tx.outputs() {
            match output {
                Output::Coin { to, .. }
                | Output::Withdrawal { to, .. }
                | Output::Change { to, .. }
                | Output::Variable { to, .. } => {
                    owners.push(to);
                }
                Output::Contract { .. } | Output::ContractCreated { .. } => {}
            }
        }

        // dedupe owners from inputs and outputs prior to indexing
        owners.sort();
        owners.dedup();

        for owner in owners {
            db.record_tx_id_owner(&owner, block_height, tx_idx as TransactionIndex, tx_id)?;
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum TransactionValidityError {
    #[allow(dead_code)]
    #[error("Coin input was already spent")]
    CoinAlreadySpent,
    #[allow(dead_code)]
    #[error("Coin has not yet reached maturity")]
    CoinHasNotMatured,
    #[allow(dead_code)]
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
    #[error("missing transaction data for tx {transaction_id:?} in block {block_id:?}")]
    MissingTransactionData {
        block_id: Bytes32,
        transaction_id: Bytes32,
    },
    #[error("VM execution error: {0:?}")]
    VmExecution(fuel_vm::prelude::InterpreterError),
    #[error("Execution error with backtrace")]
    Backtrace(Box<FuelBacktrace>),
    #[error("Transaction doesn't match expected result")]
    InvalidTransactionOutcome,
}

impl From<FuelBacktrace> for Error {
    fn from(e: FuelBacktrace) -> Self {
        Error::Backtrace(Box::new(e))
    }
}

impl From<crate::database::KvStoreError> for Error {
    fn from(e: KvStoreError) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}

impl From<InterpreterError> for Error {
    fn from(e: InterpreterError) -> Self {
        Error::VmExecution(e)
    }
}

impl From<crate::state::Error> for Error {
    fn from(e: crate::state::Error) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}
