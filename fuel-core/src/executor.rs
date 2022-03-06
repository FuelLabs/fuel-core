use crate::{
    database::{transaction::TransactionIndex, Database, KvStoreError},
    model::{
        coin::{Coin, CoinStatus},
        fuel_block::{BlockHeight, FuelBlock, FuelBlockDb},
    },
    service::Config,
    tx_pool::TransactionStatus,
};
use fuel_asm::Word;
use fuel_merkle::{binary::MerkleTree, common::StorageMap};
use fuel_storage::Storage;
use fuel_tx::{
    Address, AssetId, Bytes32, Input, Output, Receipt, Transaction, UtxoId, ValidationError,
};
use fuel_types::{bytes::SerializableVec, ContractId};
use fuel_vm::{
    consts::REG_SP,
    prelude::{Backtrace as FuelBacktrace, Interpreter},
};
use std::{
    error::Error as StdError,
    ops::{Deref, DerefMut},
};
use thiserror::Error;
use tracing::{debug, warn};

///! The executor is used for block production and validation. Given a block, it will execute all
/// the transactions contained in the block and persist changes to the underlying database as needed.
/// In production mode, block fields like transaction commitments are set based on the executed txs.
/// In validation mode, the processed block commitments are compared with the proposed block.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionMode {
    Production,
    #[allow(dead_code)]
    Validation,
}

pub struct Executor {
    pub database: Database,
    pub config: Config,
}

impl Executor {
    pub async fn execute(&self, block: &mut FuelBlock, mode: ExecutionMode) -> Result<(), Error> {
        // Compute the block id before execution, if mode is set to production just use zeroed id.
        let pre_exec_block_id = match mode {
            ExecutionMode::Production => Default::default(),
            ExecutionMode::Validation => block.id(),
        };

        let mut block_db_transaction = self.database.transaction();
        let mut storage = StorageMap::new();
        let mut txs_merkle = MerkleTree::new(&mut storage);
        let mut tx_status = vec![];
        let mut coinbase = 0u64;

        for (idx, tx) in block.transactions.iter_mut().enumerate() {
            let tx_id = tx.id();

            // Throw a clear error if the transaction id is a duplicate
            if Storage::<Bytes32, Transaction>::contains_key(
                block_db_transaction.deref_mut(),
                &tx_id,
            )? {
                return Err(Error::TransactionIdCollision(tx_id));
            }

            if self.config.utxo_validation {
                // validate utxos exist and maturity is properly set
                self.verify_input_state(block_db_transaction.deref(), tx, block.header.height)?;
                // validate transaction signature
                tx.validate_input_signature()
                    .map_err(TransactionValidityError::from)?;
            }

            self.compute_contract_input_utxo_ids(tx, &mode, block_db_transaction.deref())?;

            // verify that the tx has enough gas to cover committed costs
            self.verify_gas(tx)?;

            // index owners of inputs and outputs with tx-id, regardless of validity (hence block_tx instead of tx_db)
            self.persist_owners_index(
                block.header.height,
                tx,
                &tx_id,
                idx,
                block_db_transaction.deref_mut(),
            )?;

            // execute transaction
            // setup database view that only lives for the duration of vm execution
            let mut sub_block_db_commit = block_db_transaction.transaction();
            let sub_db_view = sub_block_db_commit.deref_mut();
            // execution vm
            let mut vm = Interpreter::with_storage(sub_db_view.clone());
            let vm_result = vm
                .transact(tx.clone())
                .map_err(|error| Error::VmExecution {
                    error,
                    transaction_id: tx_id,
                })?
                .into_owned();

            // only commit state changes if execution was a success
            if !vm_result.should_revert() {
                sub_block_db_commit.commit()?;
            }

            // update block commitment
            let tx_fee = self.total_fee_paid(tx, vm_result.receipts())?;
            coinbase = coinbase.checked_add(tx_fee).ok_or(Error::FeeOverflow)?;

            // include the canonical serialization of the malleated tx into the commitment,
            // including all witness data.
            //
            // TODO: reference the bytes directly from VM memory to save serialization. This isn't
            //       possible atm because the change output values are set on the tx instance in the vm
            //       and not also on the in-memory representation of the tx.
            let tx_bytes = vm_result.tx().clone().to_bytes();
            txs_merkle
                .push(&tx_bytes)
                .expect("In-memory impl should be infallible");

            match mode {
                ExecutionMode::Validation => {
                    // ensure tx matches vm output exactly
                    if vm_result.tx() != tx {
                        return Err(Error::InvalidTransactionOutcome {
                            transaction_id: tx_id,
                        });
                    }
                }
                ExecutionMode::Production => {
                    // malleate the block with the resultant tx from the vm
                    *tx = vm_result.tx().clone()
                }
            }

            // Store tx into the block db transaction
            Storage::<Bytes32, Transaction>::insert(
                block_db_transaction.deref_mut(),
                &tx_id,
                vm_result.tx(),
            )?;

            // change the spent status of the tx inputs
            self.spend_inputs(vm_result.tx(), block_db_transaction.deref_mut())?;

            // persist any outputs
            self.persist_outputs(
                block.header.height,
                vm_result.tx(),
                &tx_id,
                block_db_transaction.deref_mut(),
            )?;

            // persist receipts
            self.persist_receipts(
                &tx_id,
                vm_result.receipts(),
                block_db_transaction.deref_mut(),
            )?;

            let status = if vm_result.should_revert() {
                self.log_backtrace(&vm, vm_result.receipts());
                // if script result exists, log reason
                if let Some((script_result, _)) = vm_result.receipts().iter().find_map(|r| {
                    if let Receipt::ScriptResult { result, gas_used } = r {
                        Some((result, gas_used))
                    } else {
                        None
                    }
                }) {
                    TransactionStatus::Failed {
                        block_id: Default::default(),
                        time: block.header.time,
                        reason: format!("{:?}", script_result.reason()),
                        result: Some(*vm_result.state()),
                    }
                }
                // otherwise just log the revert arg
                else {
                    TransactionStatus::Failed {
                        block_id: Default::default(),
                        time: block.header.time,
                        reason: format!("{:?}", vm_result.state()),
                        result: Some(*vm_result.state()),
                    }
                }
            } else {
                // else tx was a success
                TransactionStatus::Success {
                    block_id: Default::default(),
                    time: block.header.time,
                    result: *vm_result.state(),
                }
            };

            // queue up status for this tx to be stored once block id is finalized.
            tx_status.push((tx_id, status));
        }

        // check or set transaction commitment
        let txs_root = txs_merkle
            .root()
            .expect("In-memory impl should be infallible")
            .into();
        match mode {
            ExecutionMode::Production => {
                block.header.transactions_root = txs_root;
            }
            ExecutionMode::Validation => {
                if block.header.transactions_root != txs_root {
                    return Err(Error::InvalidTransactionRoot);
                }
            }
        }

        let finalized_block_id = block.id();

        debug!("Block {:#x} fees: {}", pre_exec_block_id, coinbase);

        // check if block id doesn't match proposed block id
        if mode == ExecutionMode::Validation && pre_exec_block_id != finalized_block_id {
            // In theory this shouldn't happen since any deviance in the block should've already
            // been checked by now.
            return Err(Error::InvalidBlockId);
        }

        // save the status for every transaction using the finalized block id
        self.persist_transaction_status(
            finalized_block_id,
            &mut tx_status,
            block_db_transaction.deref_mut(),
        )?;

        // insert block into database
        Storage::<Bytes32, FuelBlockDb>::insert(
            block_db_transaction.deref_mut(),
            &finalized_block_id,
            &block.to_db_block(),
        )?;
        block_db_transaction.commit()?;
        Ok(())
    }

    // Waiting until accounts and genesis block setup is working
    fn verify_input_state(
        &self,
        db: &Database,
        transaction: &Transaction,
        block_height: BlockHeight,
    ) -> Result<(), TransactionValidityError> {
        for input in transaction.inputs() {
            match input {
                Input::Coin { utxo_id, .. } => {
                    if let Some(coin) = Storage::<UtxoId, Coin>::get(db, utxo_id)? {
                        if coin.status == CoinStatus::Spent {
                            return Err(TransactionValidityError::CoinAlreadySpent);
                        }
                        if block_height < coin.block_created + coin.maturity {
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

    /// Mark inputs as spent
    fn spend_inputs(&self, tx: &Transaction, db: &mut Database) -> Result<(), Error> {
        for input in tx.inputs() {
            if let Input::Coin {
                utxo_id,
                owner,
                amount,
                asset_id,
                maturity,
                ..
            } = input
            {
                let block_created = if self.config.utxo_validation {
                    Storage::<UtxoId, Coin>::get(db, utxo_id)?
                        .ok_or(Error::TransactionValidity(
                            TransactionValidityError::CoinDoesntExist,
                        ))?
                        .block_created
                } else {
                    // if utxo validation is disabled, just assign this new input to the original block
                    Default::default()
                };

                Storage::<UtxoId, Coin>::insert(
                    db,
                    utxo_id,
                    &Coin {
                        owner: *owner,
                        amount: *amount,
                        asset_id: *asset_id,
                        maturity: (*maturity).into(),
                        status: CoinStatus::Spent,
                        block_created,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// verify that the transaction has enough gas to cover fees
    fn verify_gas(&self, tx: &Transaction) -> Result<(), Error> {
        if tx.gas_price() != 0 || tx.byte_price() != 0 {
            let gas: Word = tx
                .inputs()
                .iter()
                .filter_map(|input| {
                    if let Input::Coin { amount, .. } = input {
                        Some(*amount)
                    } else {
                        None
                    }
                })
                .sum();
            let spent_gas: Word = tx
                .outputs()
                .iter()
                .filter_map(|output| match output {
                    Output::Coin {
                        amount, asset_id, ..
                    } if asset_id == &AssetId::default() => Some(amount),
                    Output::Withdrawal {
                        amount, asset_id, ..
                    } if asset_id == &AssetId::default() => Some(amount),
                    _ => None,
                })
                .sum();
            let byte_fees = tx.metered_bytes_size() as Word * tx.byte_price();
            let gas_fees = tx.gas_limit() * tx.gas_price();
            let total_gas_required = spent_gas
                .checked_add(byte_fees)
                .ok_or(Error::FeeOverflow)?
                .checked_add(gas_fees)
                .ok_or(Error::FeeOverflow)?;
            gas.checked_sub(total_gas_required)
                .ok_or(Error::InsufficientGas {
                    provided: gas,
                    required: total_gas_required,
                })?;
        }

        Ok(())
    }

    fn total_fee_paid(&self, tx: &Transaction, receipts: &[Receipt]) -> Result<Word, Error> {
        let mut fee = tx.metered_bytes_size() as Word * tx.byte_price();

        for r in receipts {
            if let Receipt::ScriptResult { gas_used, .. } = r {
                fee = fee.checked_add(*gas_used).ok_or(Error::FeeOverflow)?;
            }
        }

        Ok(fee)
    }

    /// In production mode, lookup and set the proper utxo ids for contract inputs
    /// In validation mode, verify the proposed utxo ids on contract inputs match the expected values.
    fn compute_contract_input_utxo_ids(
        &self,
        tx: &mut Transaction,
        mode: &ExecutionMode,
        db: &Database,
    ) -> Result<(), Error> {
        if let Transaction::Script { inputs, .. } = tx {
            for input in inputs {
                if let Input::Contract {
                    utxo_id,
                    contract_id,
                    ..
                } = input
                {
                    let maybe_utxo_id = Storage::<ContractId, UtxoId>::get(db, contract_id)?;
                    let expected_utxo_id = if self.config.utxo_validation {
                        maybe_utxo_id
                            .ok_or(Error::ContractUtxoMissing(*contract_id))?
                            .into_owned()
                    } else {
                        maybe_utxo_id.unwrap_or_default().into_owned()
                    };

                    match mode {
                        ExecutionMode::Production => *utxo_id = expected_utxo_id,
                        ExecutionMode::Validation => {
                            if *utxo_id != expected_utxo_id {
                                return Err(Error::InvalidTransactionOutcome {
                                    transaction_id: tx.id(),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Log a VM backtrace if configured to do so
    fn log_backtrace(&self, vm: &Interpreter<Database>, receipts: &[Receipt]) {
        if self.config.vm.backtrace {
            if let Some(backtrace) = receipts
                .iter()
                .find_map(Receipt::result)
                .copied()
                .map(|result| FuelBacktrace::from_vm_error(vm, result))
            {
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
        tx_id: &Bytes32,
        db: &mut Database,
    ) -> Result<(), Error> {
        for (output_index, output) in tx.outputs().iter().enumerate() {
            let utxo_id = UtxoId::new(*tx_id, output_index as u8);
            match output {
                Output::Coin {
                    amount,
                    asset_id,
                    to,
                } => Executor::insert_coin(block_height.into(), utxo_id, amount, asset_id, to, db)?,
                Output::Contract {
                    input_index: input_idx,
                    ..
                } => {
                    if let Some(Input::Contract { contract_id, .. }) =
                        tx.inputs().get(*input_idx as usize)
                    {
                        Storage::<ContractId, UtxoId>::insert(db, contract_id, &utxo_id)?;
                    } else {
                        return Err(Error::TransactionValidity(
                            TransactionValidityError::InvalidContractInputIndex(utxo_id),
                        ));
                    }
                }
                Output::Withdrawal { .. } => {
                    // TODO: Handle withdrawals somehow (new field on the block type?)
                }
                Output::Change {
                    to,
                    asset_id,
                    amount,
                } => Executor::insert_coin(block_height.into(), utxo_id, amount, asset_id, to, db)?,
                Output::Variable {
                    to,
                    asset_id,
                    amount,
                } => Executor::insert_coin(block_height.into(), utxo_id, amount, asset_id, to, db)?,
                Output::ContractCreated { contract_id, .. } => {
                    Storage::<ContractId, UtxoId>::insert(db, contract_id, &utxo_id)?;
                }
            }
        }
        Ok(())
    }

    fn insert_coin(
        fuel_height: u32,
        utxo_id: UtxoId,
        amount: &Word,
        asset_id: &AssetId,
        to: &Address,
        db: &mut Database,
    ) -> Result<(), Error> {
        let coin = Coin {
            owner: *to,
            amount: *amount,
            asset_id: *asset_id,
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
            db.record_tx_id_owner(owner, block_height, tx_idx as TransactionIndex, tx_id)?;
        }

        Ok(())
    }

    fn persist_transaction_status(
        &self,
        finalized_block_id: Bytes32,
        tx_status: &mut [(Bytes32, TransactionStatus)],
        db: &Database,
    ) -> Result<(), Error> {
        for (tx_id, status) in tx_status {
            match status {
                TransactionStatus::Submitted { .. } => {}
                TransactionStatus::Success { block_id, .. } => {
                    *block_id = finalized_block_id;
                }
                TransactionStatus::Failed { block_id, .. } => {
                    *block_id = finalized_block_id;
                }
            }
            db.update_tx_status(tx_id, status.clone())?;
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
    #[error("Contract output index isn't valid: {0:#x}")]
    InvalidContractInputIndex(UtxoId),
    #[error("Transaction validity: {0:#?}")]
    Validation(#[from] ValidationError),
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
    #[error("Transaction id was already used: {0:#x}")]
    TransactionIdCollision(Bytes32),
    #[error("output already exists")]
    OutputAlreadyExists,
    #[error("Transaction doesn't include enough value to pay for gas: {provided} < {required}")]
    InsufficientGas { provided: Word, required: Word },
    #[error("The computed fee caused an integer overflow")]
    FeeOverflow,
    #[error("Invalid transaction: {0}")]
    TransactionValidity(#[from] TransactionValidityError),
    #[error("corrupted block state")]
    CorruptedBlockState(Box<dyn StdError>),
    #[error("missing transaction data for tx {transaction_id:#x} in block {block_id:#x}")]
    MissingTransactionData {
        block_id: Bytes32,
        transaction_id: Bytes32,
    },
    #[error("Transaction({transaction_id:#x}) execution error: {error:?}")]
    VmExecution {
        error: fuel_vm::prelude::InterpreterError,
        transaction_id: Bytes32,
    },
    #[error("Execution error with backtrace")]
    Backtrace(Box<FuelBacktrace>),
    #[error("Transaction doesn't match expected result: {transaction_id:#x}")]
    InvalidTransactionOutcome { transaction_id: Bytes32 },
    #[error("Transaction root is invalid")]
    InvalidTransactionRoot,
    #[error("The amount of charged fees is invalid")]
    InvalidFeeAmount,
    #[error("Block id is invalid")]
    InvalidBlockId,
    #[error("No matching utxo for contract id ${0:#x}")]
    ContractUtxoMissing(ContractId),
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

impl From<crate::state::Error> for Error {
    fn from(e: crate::state::Error) -> Self {
        Error::CorruptedBlockState(Box::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fuel_block::FuelBlockHeader;
    use chrono::{TimeZone, Utc};
    use fuel_asm::Opcode;
    use fuel_crypto::SecretKey;
    use fuel_tx::TransactionBuilder;
    use fuel_types::{ContractId, Salt};
    use fuel_vm::consts::{REG_ONE, REG_ZERO};
    use fuel_vm::util::test_helpers::TestBuilder as TxBuilder;
    use itertools::Itertools;
    use rand::prelude::StdRng;
    use rand::{Rng, SeedableRng};

    fn test_block(num_txs: usize) -> FuelBlock {
        let transactions = (1..num_txs + 1)
            .into_iter()
            .map(|i| {
                TxBuilder::new(2322u64)
                    .gas_limit(10)
                    .coin_input(AssetId::default(), (i as Word) * 100)
                    .coin_output(AssetId::default(), (i as Word) * 50)
                    .change_output(AssetId::default())
                    .build()
            })
            .collect_vec();

        FuelBlock {
            header: Default::default(),
            transactions,
        }
    }

    fn create_contract<R: Rng>(contract_code: Vec<u8>, rng: &mut R) -> (Transaction, ContractId) {
        let salt: Salt = rng.gen();
        let contract = fuel_vm::contract::Contract::from(contract_code);
        let root = contract.root();
        let state_root = fuel_vm::contract::Contract::default_state_root();
        let contract_id = contract.id(&salt, &root, &state_root);

        let tx = Transaction::create(
            0,
            0,
            0,
            0,
            0,
            salt,
            vec![],
            vec![],
            vec![],
            vec![Output::ContractCreated {
                contract_id,
                state_root,
            }],
            vec![Default::default()],
        );
        (tx, contract_id)
    }

    // Happy path test case that a produced block will also validate
    #[tokio::test]
    async fn executor_validates_correctly_produced_block() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let mut block = test_block(10);

        producer
            .execute(&mut block, ExecutionMode::Production)
            .await
            .unwrap();

        let validation_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;
        assert!(validation_result.is_ok());
    }

    // Ensure transaction commitment != default after execution
    #[tokio::test]
    async fn executor_commits_transactions_to_block() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let mut block = test_block(10);
        let start_block = block.clone();

        producer
            .execute(&mut block, ExecutionMode::Production)
            .await
            .unwrap();

        assert_ne!(
            start_block.header.transactions_root,
            block.header.transactions_root
        )
    }

    // Ensure tx has at least one input to cover gas
    #[tokio::test]
    async fn executor_invalidates_missing_gas_input() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let gas_limit = 100;
        let gas_price = 1;
        let mut tx = Transaction::default();
        tx.set_gas_limit(gas_limit);
        tx.set_gas_price(gas_price);

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let produce_result = producer
            .execute(&mut block, ExecutionMode::Production)
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::InsufficientGas { required, .. }) if required == gas_limit
        ));

        let verify_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::InsufficientGas {required, ..}) if required == gas_limit
        ))
    }

    #[tokio::test]
    async fn executor_invalidates_duplicate_tx_id() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![Transaction::default(), Transaction::default()],
        };

        let produce_result = producer
            .execute(&mut block, ExecutionMode::Production)
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::TransactionIdCollision(_))
        ));

        let verify_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::TransactionIdCollision(_))
        ));
    }

    // invalidate a block if a tx input contains a previously used txo
    #[tokio::test]
    async fn executor_invalidates_spent_inputs() {
        let mut rng = StdRng::seed_from_u64(2322u64);

        let spent_utxo_id = rng.gen();
        let owner = Default::default();
        let amount = 10;
        let asset_id = Default::default();
        let maturity = Default::default();
        let block_created = Default::default();
        let coin = Coin {
            owner,
            amount,
            asset_id,
            maturity,
            status: CoinStatus::Spent,
            block_created,
        };

        let mut db = Database::default();
        // initialize database with coin that was already spent
        Storage::<UtxoId, Coin>::insert(&mut db, &spent_utxo_id, &coin).unwrap();

        // create an input referring to a coin that is already spent
        let input = Input::coin(spent_utxo_id, owner, amount, asset_id, 0, 0, vec![], vec![]);
        let output = Output::Change {
            to: owner,
            amount: 0,
            asset_id,
        };
        let tx = Transaction::script(
            0,
            0,
            0,
            0,
            vec![],
            vec![],
            vec![input],
            vec![output],
            vec![Default::default()],
        );

        // setup executor with utxo-validation enabled
        let config = Config {
            utxo_validation: true,
            ..Config::local_node()
        };
        let producer = Executor {
            database: db.clone(),
            config: config.clone(),
        };

        let verifier = Executor {
            database: db.clone(),
            config: config.clone(),
        };

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let produce_result = producer
            .execute(&mut block, ExecutionMode::Production)
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinAlreadySpent
            ))
        ));

        let verify_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinAlreadySpent
            ))
        ));
    }

    // invalidate a block if a tx input doesn't exist
    #[tokio::test]
    async fn executor_invalidates_missing_inputs() {
        // create an input which doesn't exist in the utxo set
        let mut rng = StdRng::seed_from_u64(2322u64);

        let tx =
            TransactionBuilder::script(vec![Opcode::RET(REG_ONE)].into_iter().collect(), vec![])
                .add_unsigned_coin_input(
                    rng.gen(),
                    &SecretKey::random(&mut rng),
                    10,
                    Default::default(),
                    0,
                    vec![],
                    vec![],
                )
                .add_output(Output::Change {
                    to: Default::default(),
                    amount: 0,
                    asset_id: Default::default(),
                })
                .finalize();

        // setup executors with utxo-validation enabled
        let config = Config {
            utxo_validation: true,
            ..Config::local_node()
        };
        let producer = Executor {
            database: Database::default(),
            config: config.clone(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: config.clone(),
        };

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let produce_result = producer
            .execute(&mut block, ExecutionMode::Production)
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinDoesntExist
            ))
        ));

        let verify_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinDoesntExist
            ))
        ));
    }

    // corrupt a produced block by randomizing change amount
    // and verify that the executor invalidates the tx
    #[tokio::test]
    async fn executor_invalidates_blocks_with_diverging_tx_outputs() {
        let input_amount = 10;
        let fake_output_amount = 100;

        let tx = TxBuilder::new(2322u64)
            .gas_limit(1)
            .coin_input(Default::default(), input_amount)
            .change_output(Default::default())
            .build();

        let tx_id = tx.id();

        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        producer
            .execute(&mut block, ExecutionMode::Production)
            .await
            .unwrap();

        // modify change amount
        if let Transaction::Script { outputs, .. } = &mut block.transactions[0] {
            if let Output::Change { amount, .. } = &mut outputs[0] {
                *amount = fake_output_amount
            }
        }

        let verify_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::InvalidTransactionOutcome { transaction_id }) if transaction_id == tx_id
        ));
    }

    // corrupt the merkle sum tree commitment from a produced block and verify that the
    // validation logic will reject the block
    #[tokio::test]
    async fn executor_invalidates_blocks_with_diverging_tx_commitment() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let tx = TxBuilder::new(2322u64)
            .gas_limit(1)
            .coin_input(Default::default(), 10)
            .change_output(Default::default())
            .build();

        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        producer
            .execute(&mut block, ExecutionMode::Production)
            .await
            .unwrap();

        // randomize transaction commitment
        block.header.transactions_root = rng.gen();

        let verify_result = verifier
            .execute(&mut block, ExecutionMode::Validation)
            .await;

        assert!(matches!(verify_result, Err(Error::InvalidTransactionRoot)))
    }

    #[tokio::test]
    async fn input_coins_are_marked_as_spent() {
        // ensure coins are marked as spent after tx is processed
        let tx = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build();

        let db = Database::default();
        let executor = Executor {
            database: db.clone(),
            config: Config::local_node(),
        };

        let mut block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        executor
            .execute(&mut block, ExecutionMode::Production)
            .await
            .unwrap();

        // assert the tx coin is spent
        let coin = Storage::<UtxoId, Coin>::get(&db, block.transactions[0].inputs()[0].utxo_id())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Spent);
    }

    #[tokio::test]
    async fn input_coins_are_marked_as_spent_with_utxo_validation_enabled() {
        // ensure coins are marked as spent after tx is processed
        let mut rng = StdRng::seed_from_u64(2322u64);
        let starting_block = BlockHeight::from(5u64);

        let tx =
            TransactionBuilder::script(vec![Opcode::RET(REG_ONE)].into_iter().collect(), vec![])
                .add_unsigned_coin_input(
                    rng.gen(),
                    &SecretKey::random(&mut rng),
                    100,
                    Default::default(),
                    0,
                    vec![],
                    vec![],
                )
                .add_output(Output::Change {
                    to: Default::default(),
                    amount: 0,
                    asset_id: Default::default(),
                })
                .finalize();
        let mut db = Database::default();

        // insert coin into state
        if let Input::Coin {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        } = tx.inputs()[0]
        {
            Storage::<UtxoId, Coin>::insert(
                &mut db,
                &utxo_id,
                &Coin {
                    owner,
                    amount,
                    asset_id,
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: starting_block,
                },
            )
            .unwrap();
        }

        let executor = Executor {
            database: db.clone(),
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        };

        let mut block = FuelBlock {
            header: FuelBlockHeader {
                height: 6u64.into(),
                number: Default::default(),
                parent_hash: Default::default(),
                time: Utc.timestamp(0, 0),
                producer: Default::default(),
                transactions_root: Default::default(),
                prev_root: Default::default(),
            },
            transactions: vec![tx],
        };

        executor
            .execute(&mut block, ExecutionMode::Production)
            .await
            .unwrap();

        // assert the tx coin is spent
        let coin = Storage::<UtxoId, Coin>::get(&db, block.transactions[0].inputs()[0].utxo_id())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Spent);
        // assert block created from coin before spend is still intact (only a concern when utxo-validation is enabled)
        assert_eq!(coin.block_created, starting_block)
    }

    #[tokio::test]
    async fn validation_succeeds_when_input_contract_utxo_id_uses_expected_value() {
        let mut rng = StdRng::seed_from_u64(2322);
        // create a contract in block 1
        // verify a block 2 with tx containing contract id from block 1, using the correct contract utxo_id from block 1.
        let (tx, contract_id) = create_contract(vec![], &mut rng);
        let mut first_block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let tx2 = TxBuilder::new(2322)
            .script(vec![Opcode::RET(1)])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build();
        let mut second_block = FuelBlock {
            header: FuelBlockHeader {
                height: 2u64.into(),
                ..Default::default()
            },
            transactions: vec![tx2],
        };

        let db = Database::default();

        let setup = Executor {
            database: db.clone(),
            config: Config::local_node(),
        };

        setup
            .execute(&mut first_block, ExecutionMode::Production)
            .await
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor {
            database: producer_view,
            config: Config::local_node(),
        };
        producer
            .execute(&mut second_block, ExecutionMode::Production)
            .await
            .unwrap();

        let verifier = Executor {
            database: db,
            config: Config::local_node(),
        };
        let verify_result = verifier
            .execute(&mut second_block, ExecutionMode::Validation)
            .await;
        assert!(verify_result.is_ok());
    }

    // verify that a contract input must exist for a transaction
    #[tokio::test]
    async fn invalidates_if_input_contract_utxo_id_is_divergent() {
        let mut rng = StdRng::seed_from_u64(2322);

        // create a contract in block 1
        // verify a block 2 containing contract id from block 1, with wrong input contract utxo_id
        let (tx, contract_id) = create_contract(vec![], &mut rng);
        let tx2 = TxBuilder::new(2322)
            .script(vec![Opcode::ADDI(0x10, REG_ZERO, 0), Opcode::RET(1)])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build();

        let mut first_block = FuelBlock {
            header: Default::default(),
            transactions: vec![tx, tx2],
        };

        let tx3 = TxBuilder::new(2322)
            .script(vec![Opcode::ADDI(0x10, REG_ZERO, 1), Opcode::RET(1)])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build();
        let tx_id = tx3.id();

        let mut second_block = FuelBlock {
            header: FuelBlockHeader {
                height: 2u64.into(),
                ..Default::default()
            },
            transactions: vec![tx3],
        };

        let db = Database::default();

        let setup = Executor {
            database: db.clone(),
            config: Config::local_node(),
        };

        setup
            .execute(&mut first_block, ExecutionMode::Production)
            .await
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor {
            database: producer_view,
            config: Config::local_node(),
        };

        producer
            .execute(&mut second_block, ExecutionMode::Production)
            .await
            .unwrap();
        // Corrupt the utxo_id of the contract output
        if let Transaction::Script { inputs, .. } = &mut second_block.transactions[0] {
            if let Input::Contract { utxo_id, .. } = &mut inputs[0] {
                // use a previously valid contract id which isn't the correct one for this block
                *utxo_id = UtxoId::new(tx_id, 0);
            }
        }

        let verifier = Executor {
            database: db,
            config: Config::local_node(),
        };
        let verify_result = verifier
            .execute(&mut second_block, ExecutionMode::Validation)
            .await;

        assert!(matches!(
            verify_result,
            Err(Error::InvalidTransactionOutcome {
                transaction_id
            }) if transaction_id == tx_id
        ));
    }
}
