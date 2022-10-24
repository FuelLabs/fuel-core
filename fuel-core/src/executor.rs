use crate::{
    database::{
        storage::{
            ContractsLatestUtxo,
            FuelBlocks,
            Receipts,
        },
        transaction::TransactionIndex,
        transactional::DatabaseTransaction,
        Database,
    },
    model::{
        BlockHeight,
        Coin,
        CoinStatus,
        FuelBlock,
    },
    service::Config,
    tx_pool::TransactionStatus,
};
use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_storage,
        fuel_tx::{
            Address,
            AssetId,
            Bytes32,
            CheckedTransaction,
            Input,
            Output,
            Receipt,
            Transaction,
            TransactionFee,
            UtxoId,
        },
        fuel_types::MessageId,
        fuel_vm::{
            consts::REG_SP,
            prelude::{
                Backtrace as FuelBacktrace,
                Interpreter,
                PredicateStorage,
            },
        },
        prelude::StorageInspect,
        state::StateTransition,
    },
    db::{
        Coins,
        Messages,
        Transactions,
    },
    executor::{
        Error,
        ExecutionBlock,
        ExecutionKind,
        ExecutionType,
        ExecutionTypes,
        Executor as ExecutorTrait,
        TransactionValidityError,
    },
    model::{
        BlockId,
        DaBlockHeight,
        Message,
        PartialFuelBlock,
    },
};
use fuel_storage::{
    StorageAsMut,
    StorageAsRef,
};
use std::ops::{
    Deref,
    DerefMut,
};
use tracing::{
    debug,
    warn,
};

/// ! The executor is used for block production and validation. Given a block, it will execute all
/// the transactions contained in the block and persist changes to the underlying database as needed.
/// In production mode, block fields like transaction commitments are set based on the executed txs.
/// In validation mode, the processed block commitments are compared with the proposed block.

pub struct Executor {
    pub database: Database,
    pub config: Config,
}

/// Data that is generated after executing all transactions.
struct ExecutionData {
    coinbase: u64,
    message_ids: Vec<MessageId>,
    tx_status: Vec<(Bytes32, TransactionStatus)>,
}

#[async_trait::async_trait]
impl ExecutorTrait for Executor {
    async fn execute(&self, block: ExecutionBlock) -> Result<FuelBlock, Error> {
        self.execute_inner(block, &self.database).await
    }

    async fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> Result<Vec<Vec<Receipt>>, Error> {
        // run the block in a temporary transaction without persisting any state
        let db_tx = self.database.transaction();
        let temporary_db = db_tx.as_ref();

        // fallback to service config value if no utxo_validation override is provided
        let utxo_validation = utxo_validation.unwrap_or(self.config.utxo_validation);

        // spawn a nested executor instance to override utxo_validation config
        let executor = Self {
            config: Config {
                utxo_validation,
                ..self.config.clone()
            },
            database: temporary_db.clone(),
        };

        let block = executor.execute_inner(block, temporary_db).await?;
        block
            .transactions()
            .iter()
            .map(|tx| {
                let id = tx.id();
                StorageInspect::<Receipts>::get(temporary_db, &id)
                    .transpose()
                    .unwrap_or_else(|| Ok(Default::default()))
                    .map(|v| v.into_owned())
            })
            .collect::<Result<Vec<Vec<Receipt>>, _>>()
            .map_err(Into::into)
        // drop `temporary_db` without committing to avoid altering state.
    }
}

impl Executor {
    #[tracing::instrument(skip(self))]
    async fn execute_inner(
        &self,
        block: ExecutionBlock,
        database: &Database,
    ) -> Result<FuelBlock, Error> {
        // Compute the block id before execution if there is one.
        let pre_exec_block_id = block.id();
        // Get the transaction root before execution if there is one.
        let pre_exec_txs_root = block.txs_root();

        // If there is full fuel block for validation then map it into
        // a partial header.
        let mut block = block.map_v(PartialFuelBlock::from);

        // Create a new database transaction.
        let mut block_db_transaction = database.transaction();

        // Execute all transactions.
        let execution_data = self
            .execute_transactions(&mut block_db_transaction, block.as_mut())
            .await?;

        let ExecutionData {
            coinbase,
            message_ids,
            mut tx_status,
        } = execution_data;

        // Now that the transactions have been executed, generate the full header.
        let block = block.map(|b| b.generate(&message_ids[..]));

        // check transaction commitment
        if let Some(pre_exec_txs_root) = pre_exec_txs_root {
            if block.header().transactions_root != pre_exec_txs_root {
                return Err(Error::InvalidTransactionRoot)
            }
        }

        let finalized_block_id = block.id();

        debug!(
            "Block {:#x} fees: {}",
            pre_exec_block_id.unwrap_or(finalized_block_id),
            coinbase
        );

        // check if block id doesn't match proposed block id
        if let Some(pre_exec_block_id) = pre_exec_block_id {
            if pre_exec_block_id != finalized_block_id {
                // In theory this shouldn't happen since any deviance in the block should've already
                // been checked by now.
                return Err(Error::InvalidBlockId)
            }
        }

        // save the status for every transaction using the finalized block id
        self.persist_transaction_status(
            finalized_block_id,
            &mut tx_status,
            block_db_transaction.deref_mut(),
        )?;

        // cleanup unfinalized headers (block height + time + producer)
        block_db_transaction
            .deref_mut()
            .storage::<FuelBlocks>()
            .remove(&Bytes32::zeroed())?;

        // insert block into database
        block_db_transaction
            .deref_mut()
            .storage::<FuelBlocks>()
            .insert(&finalized_block_id.into(), &block.to_db_block())?;

        // Commit the database transaction.
        block_db_transaction.commit()?;

        // Get the complete fuel block.
        Ok(block.into_inner())
    }

    #[tracing::instrument(skip(self))]
    /// Execute all transactions on the fuel block.
    async fn execute_transactions(
        &self,
        block_db_transaction: &mut DatabaseTransaction,
        block: ExecutionType<&mut PartialFuelBlock>,
    ) -> Result<ExecutionData, Error> {
        let mut execution_data = ExecutionData {
            coinbase: 0,
            message_ids: Vec::new(),
            tx_status: Vec::new(),
        };

        // Insert the current headers (including time, block height into the db tx)
        block_db_transaction
            .deref_mut()
            .storage::<FuelBlocks>()
            .insert(
                &Bytes32::zeroed(), // use id of zero as current block
                &block.to_partial_db_block(),
            )?;

        // Split out the execution kind and partial block.
        let (execution_kind, block) = block.split();

        // Execute each transaction.
        for (idx, tx) in block.transactions.iter_mut().enumerate() {
            let tx_id = tx.id();

            // Throw a clear error if the transaction id is a duplicate
            if block_db_transaction
                .deref_mut()
                .storage::<Transactions>()
                .contains_key(&tx_id)?
            {
                return Err(Error::TransactionIdCollision(tx_id))
            }

            // Wrap the transaction in the execution kind.
            let mut wrapped_tx: ExecutionTypes<&mut Transaction, &Transaction> =
                match execution_kind {
                    ExecutionKind::Production => ExecutionTypes::Production(tx),
                    ExecutionKind::Validation => ExecutionTypes::Validation(&*tx),
                };
            self.compute_contract_input_utxo_ids(
                &mut wrapped_tx,
                block_db_transaction.deref(),
            )?;

            let checked_tx = CheckedTransaction::check_unsigned(
                tx.clone(),
                (*block.header.height()).into(),
                &self.config.chain_conf.transaction_parameters,
            )?;
            let min_fee = checked_tx.min_fee();
            let max_fee = checked_tx.max_fee();

            self.verify_tx_predicates(&checked_tx)?;

            if self.config.utxo_validation {
                // validate transaction has at least one coin
                self.verify_tx_has_at_least_one_coin_or_message(tx)?;
                // validate utxos exist and maturity is properly set
                self.verify_input_state(
                    block_db_transaction.deref(),
                    tx,
                    *block.header.height(),
                    block.header.da_height,
                )?;
                // validate transaction signature
                tx.validate_input_signature()
                    .map_err(TransactionValidityError::from)?;
            }

            // index owners of inputs and outputs with tx-id, regardless of validity (hence block_tx instead of tx_db)
            self.persist_owners_index(
                *block.header.height(),
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
            let mut vm = Interpreter::with_storage(
                sub_db_view.clone(),
                self.config.chain_conf.transaction_parameters,
            );
            let vm_result: StateTransition = vm
                .transact(checked_tx)
                .map_err(|error| Error::VmExecution {
                    error,
                    transaction_id: tx_id,
                })?
                .into();

            // only commit state changes if execution was a success
            if !vm_result.should_revert() {
                sub_block_db_commit.commit()?;
            }

            // update block commitment
            let tx_fee = self.total_fee_paid(
                min_fee,
                max_fee,
                tx.gas_price(),
                vm_result.receipts(),
            )?;
            execution_data.coinbase = execution_data
                .coinbase
                .checked_add(tx_fee)
                .ok_or(Error::FeeOverflow)?;

            // Check or set the executed transaction.
            match execution_kind {
                ExecutionKind::Validation => {
                    // ensure tx matches vm output exactly
                    if vm_result.tx() != tx {
                        return Err(Error::InvalidTransactionOutcome {
                            transaction_id: tx_id,
                        })
                    }
                }
                ExecutionKind::Production => {
                    // malleate the block with the resultant tx from the vm
                    *tx = vm_result.tx().clone()
                }
            }

            // Store tx into the block db transaction
            block_db_transaction
                .deref_mut()
                .storage::<Transactions>()
                .insert(&tx_id, vm_result.tx())?;

            // change the spent status of the tx inputs
            self.spend_inputs(
                vm_result.tx(),
                block_db_transaction.deref_mut(),
                *block.header.height(),
                self.config.utxo_validation,
            )?;

            // persist any outputs
            self.persist_outputs(
                *block.header.height(),
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
                // get reason for revert
                let reason = vm_result
                    .receipts()
                    .iter()
                    .find_map(|receipt| match receipt {
                        // Format as `Revert($rA)`
                        Receipt::Revert { ra, .. } => Some(format!("Revert({})", ra)),
                        // Display PanicReason e.g. `OutOfGas`
                        Receipt::Panic { reason, .. } => {
                            Some(format!("{}", reason.reason()))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| format!("{:?}", vm_result.state()));

                TransactionStatus::Failed {
                    block_id: Default::default(),
                    time: *block.header.time(),
                    reason,
                    result: Some(*vm_result.state()),
                }
            } else {
                // else tx was a success
                TransactionStatus::Success {
                    block_id: Default::default(),
                    time: *block.header.time(),
                    result: *vm_result.state(),
                }
            };

            // queue up status for this tx to be stored once block id is finalized.
            execution_data.tx_status.push((tx_id, status));
            execution_data
                .message_ids
                .extend(vm_result.receipts().iter().filter_map(|r| match r {
                    Receipt::MessageOut { message_id, .. } => Some(*message_id),
                    _ => None,
                }));
        }

        Ok(execution_data)
    }

    fn verify_input_state(
        &self,
        db: &Database,
        transaction: &Transaction,
        block_height: BlockHeight,
        block_da_height: DaBlockHeight,
    ) -> Result<(), TransactionValidityError> {
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned { utxo_id, .. }
                | Input::CoinPredicate { utxo_id, .. } => {
                    if let Some(coin) = db.storage::<Coins>().get(utxo_id)? {
                        if coin.status == CoinStatus::Spent {
                            return Err(TransactionValidityError::CoinAlreadySpent(
                                *utxo_id,
                            ))
                        }
                        if block_height < coin.block_created + coin.maturity {
                            return Err(TransactionValidityError::CoinHasNotMatured(
                                *utxo_id,
                            ))
                        }
                    } else {
                        return Err(TransactionValidityError::CoinDoesNotExist(*utxo_id))
                    }
                }
                Input::Contract { .. } => {}
                Input::MessageSigned { message_id, .. }
                | Input::MessagePredicate { message_id, .. } => {
                    if let Some(message) = db.storage::<Messages>().get(message_id)? {
                        if message.fuel_block_spend.is_some() {
                            return Err(TransactionValidityError::MessageAlreadySpent(
                                *message_id,
                            ))
                        }
                        if message.da_height > block_da_height {
                            return Err(TransactionValidityError::MessageSpendTooEarly(
                                *message_id,
                            ))
                        }
                    } else {
                        return Err(TransactionValidityError::MessageDoesNotExist(
                            *message_id,
                        ))
                    }
                }
            }
        }

        Ok(())
    }

    /// Verify all the predicates of a tx.
    pub fn verify_tx_predicates(&self, tx: &CheckedTransaction) -> Result<(), Error> {
        if !Interpreter::<PredicateStorage>::check_predicates(
            tx.clone(),
            self.config.chain_conf.transaction_parameters,
        ) {
            return Err(Error::TransactionValidity(
                TransactionValidityError::InvalidPredicate(tx.transaction().id()),
            ))
        }

        Ok(())
    }

    /// Verify the transaction has at least one coin.
    ///
    /// TODO: This verification really belongs in fuel-tx, and can be removed once
    ///       https://github.com/FuelLabs/fuel-tx/issues/118 is resolved.
    fn verify_tx_has_at_least_one_coin_or_message(
        &self,
        tx: &Transaction,
    ) -> Result<(), Error> {
        if tx
            .inputs()
            .iter()
            .any(|input| input.is_coin() || input.is_message())
        {
            Ok(())
        } else {
            Err(TransactionValidityError::NoCoinOrMessageInput(tx.id()).into())
        }
    }

    /// Mark inputs as spent
    fn spend_inputs(
        &self,
        tx: &Transaction,
        db: &mut Database,
        block_height: BlockHeight,
        utxo_validation: bool,
    ) -> Result<(), Error> {
        for input in tx.inputs() {
            match input {
                Input::CoinSigned {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    maturity,
                    ..
                }
                | Input::CoinPredicate {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    maturity,
                    ..
                } => {
                    let block_created = if utxo_validation {
                        db.storage::<Coins>()
                            .get(utxo_id)?
                            .ok_or(Error::TransactionValidity(
                                TransactionValidityError::CoinDoesNotExist(*utxo_id),
                            ))?
                            .block_created
                    } else {
                        // if utxo validation is disabled, just assign this new input to the original block
                        Default::default()
                    };

                    db.storage::<Coins>().insert(
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
                Input::MessageSigned {
                    message_id,
                    sender,
                    recipient,
                    amount,
                    nonce,
                    data,
                    ..
                }
                | Input::MessagePredicate {
                    message_id,
                    sender,
                    recipient,
                    amount,
                    nonce,
                    data,
                    ..
                } => {
                    let da_height = if utxo_validation {
                        db.storage::<Messages>()
                            .get(message_id)?
                            .ok_or(Error::TransactionValidity(
                                TransactionValidityError::MessageDoesNotExist(
                                    *message_id,
                                ),
                            ))?
                            .da_height
                    } else {
                        // if utxo validation is disabled, just assignto the original block
                        Default::default()
                    };

                    db.storage::<Messages>().insert(
                        message_id,
                        &Message {
                            da_height,
                            fuel_block_spend: Some(block_height),
                            sender: *sender,
                            recipient: *recipient,
                            nonce: *nonce,
                            amount: *amount,
                            data: data.clone(),
                        },
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn total_fee_paid(
        &self,
        min_fee: u64,
        max_fee: u64,
        gas_price: u64,
        receipts: &[Receipt],
    ) -> Result<Word, Error> {
        for r in receipts {
            if let Receipt::ScriptResult { gas_used, .. } = r {
                return TransactionFee::gas_refund_value(
                    &self.config.chain_conf.transaction_parameters,
                    *gas_used,
                    gas_price,
                )
                .and_then(|refund| max_fee.checked_sub(refund))
                .ok_or(Error::FeeOverflow)
            }
        }
        // if there's no script result (i.e. create) then fee == base amount
        Ok(min_fee)
    }

    /// In production mode, lookup and set the proper utxo ids for contract inputs
    /// In validation mode, verify the proposed utxo ids on contract inputs match the expected values.
    fn compute_contract_input_utxo_ids(
        &self,
        tx: &mut ExecutionTypes<&mut Transaction, &Transaction>,
        db: &Database,
    ) -> Result<(), Error> {
        let expected_utxo_id = |contract_id| {
            let maybe_utxo_id = db.storage::<ContractsLatestUtxo>().get(contract_id)?;
            let expected_utxo_id = if self.config.utxo_validation {
                maybe_utxo_id
                    .ok_or(Error::ContractUtxoMissing(*contract_id))?
                    .into_owned()
            } else {
                maybe_utxo_id.unwrap_or_default().into_owned()
            };
            Result::<_, Error>::Ok(expected_utxo_id)
        };
        match tx {
            ExecutionTypes::Production(Transaction::Script { inputs, .. }) => {
                let iter = inputs.iter_mut().filter_map(|input| match input {
                    Input::Contract {
                        ref mut utxo_id,
                        ref contract_id,
                        ..
                    } => Some((utxo_id, contract_id)),
                    _ => None,
                });
                for (utxo_id, contract_id) in iter {
                    *utxo_id = expected_utxo_id(contract_id)?;
                }
            }
            // Needed to convince the compiler that tx is taken by ref here
            #[allow(clippy::needless_borrow)]
            ExecutionTypes::Validation(ref tx @ Transaction::Script { inputs, .. }) => {
                let iter = inputs.iter().filter_map(|input| match input {
                    Input::Contract {
                        utxo_id,
                        contract_id,
                        ..
                    } => Some((utxo_id, contract_id)),
                    _ => None,
                });
                for (utxo_id, contract_id) in iter {
                    if *utxo_id != expected_utxo_id(contract_id)? {
                        return Err(Error::InvalidTransactionOutcome {
                            transaction_id: tx.id(),
                        })
                    }
                }
            }
            _ => (),
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
                } => Executor::insert_coin(
                    block_height.into(),
                    utxo_id,
                    amount,
                    asset_id,
                    to,
                    db,
                )?,
                Output::Contract {
                    input_index: input_idx,
                    ..
                } => {
                    if let Some(Input::Contract { contract_id, .. }) =
                        tx.inputs().get(*input_idx as usize)
                    {
                        db.storage::<ContractsLatestUtxo>()
                            .insert(contract_id, &utxo_id)?;
                    } else {
                        return Err(Error::TransactionValidity(
                            TransactionValidityError::InvalidContractInputIndex(utxo_id),
                        ))
                    }
                }
                Output::Message { .. } => {
                    // TODO: Handle message outputs somehow (new field on the block type?)
                }
                Output::Change {
                    to,
                    asset_id,
                    amount,
                } => Executor::insert_coin(
                    block_height.into(),
                    utxo_id,
                    amount,
                    asset_id,
                    to,
                    db,
                )?,
                Output::Variable {
                    to,
                    asset_id,
                    amount,
                } => Executor::insert_coin(
                    block_height.into(),
                    utxo_id,
                    amount,
                    asset_id,
                    to,
                    db,
                )?,
                Output::ContractCreated { contract_id, .. } => {
                    db.storage::<ContractsLatestUtxo>()
                        .insert(contract_id, &utxo_id)?;
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
        // Only insert a coin output if it has some amount.
        // This is because variable or transfer outputs won't have any value
        // if there's a revert or panic and shouldn't be added to the utxo set.
        if *amount > Word::MIN {
            let coin = Coin {
                owner: *to,
                amount: *amount,
                asset_id: *asset_id,
                maturity: 0u32.into(),
                status: CoinStatus::Unspent,
                block_created: fuel_height.into(),
            };

            if db.storage::<Coins>().insert(&utxo_id, &coin)?.is_some() {
                return Err(Error::OutputAlreadyExists)
            }
        }

        Ok(())
    }

    fn persist_receipts(
        &self,
        tx_id: &Bytes32,
        receipts: &[Receipt],
        db: &mut Database,
    ) -> Result<(), Error> {
        if db
            .storage::<Receipts>()
            .insert(tx_id, &Vec::from(receipts))?
            .is_some()
        {
            return Err(Error::OutputAlreadyExists)
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
            if let Input::CoinSigned { owner, .. } | Input::CoinPredicate { owner, .. } =
                input
            {
                owners.push(owner);
            }
        }

        for output in tx.outputs() {
            match output {
                Output::Coin { to, .. }
                | Output::Message { recipient: to, .. }
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
            db.record_tx_id_owner(
                owner,
                block_height,
                tx_idx as TransactionIndex,
                tx_id,
            )?;
        }

        Ok(())
    }

    fn persist_transaction_status(
        &self,
        finalized_block_id: BlockId,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{
        TimeZone,
        Utc,
    };
    use fuel_core_interfaces::{
        common::{
            fuel_asm::Opcode,
            fuel_crypto::SecretKey,
            fuel_tx::{
                self,
                ConsensusParameters,
                Transaction,
                TransactionBuilder,
                ValidationError,
            },
            fuel_types::{
                bytes::SerializableVec,
                ContractId,
                Immediate12,
                Immediate18,
                Salt,
            },
            fuel_vm::{
                consts::{
                    REG_CGAS,
                    REG_FP,
                    REG_ONE,
                    REG_ZERO,
                },
                prelude::{
                    Call,
                    CallFrame,
                },
                script_with_data_offset,
                util::test_helpers::TestBuilder as TxBuilder,
            },
        },
        executor::ExecutionTypes,
        model::{
            CheckedMessage,
            DaBlockHeight,
            FuelConsensusHeader,
            Message,
            PartialFuelBlockHeader,
        },
        relayer::RelayerDb,
    };
    use itertools::Itertools;
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };

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
                    .transaction()
                    .clone()
            })
            .collect_vec();

        let mut block = FuelBlock::default();
        *block.transactions_mut() = transactions;
        block
    }

    fn create_contract<R: Rng>(
        contract_code: Vec<u8>,
        rng: &mut R,
    ) -> (Transaction, ContractId) {
        let salt: Salt = rng.gen();
        let contract = fuel_tx::Contract::from(contract_code.clone());
        let root = contract.root();
        let state_root = fuel_tx::Contract::default_state_root();
        let contract_id = contract.id(&salt, &root, &state_root);

        let tx = Transaction::create(
            0,
            0,
            0,
            0,
            salt,
            vec![],
            vec![],
            vec![Output::ContractCreated {
                contract_id,
                state_root,
            }],
            vec![contract_code.into()],
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
        let block = test_block(10);

        let block = producer
            .execute(ExecutionTypes::Production(block.into()))
            .await
            .unwrap();

        let validation_result = verifier.execute(ExecutionTypes::Validation(block)).await;
        assert!(validation_result.is_ok());
    }

    // Ensure transaction commitment != default after execution
    #[tokio::test]
    async fn executor_commits_transactions_to_block() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let block = test_block(10);
        let start_block = block.clone();

        let block = producer
            .execute(ExecutionBlock::Production(block.into()))
            .await
            .unwrap();

        assert_ne!(
            start_block.header().transactions_root,
            block.header().transactions_root
        )
    }

    // Ensure tx has at least one input to cover gas
    #[tokio::test]
    async fn executor_invalidates_missing_gas_input() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let factor = producer
            .config
            .chain_conf
            .transaction_parameters
            .gas_price_factor as f64;

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let gas_limit = 100;
        let gas_price = 1;
        let mut tx = Transaction::default();
        tx.set_gas_limit(gas_limit);
        tx.set_gas_price(gas_price);

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let mut block_db_transaction = producer.database.transaction();
        let produce_result = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::InvalidTransaction(ValidationError::InsufficientFeeAmount { expected, .. })) if expected == (gas_limit as f64 / factor).ceil() as u64
        ));

        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::InvalidTransaction(ValidationError::InsufficientFeeAmount { expected, ..})) if expected == (gas_limit as f64 / factor).ceil() as u64
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

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![Transaction::default(), Transaction::default()],
        };

        let mut block_db_transaction = producer.database.transaction();
        let produce_result = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::TransactionIdCollision(_))
        ));

        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
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

        let db = &mut Database::default();
        // initialize database with coin that was already spent
        db.storage::<Coins>().insert(&spent_utxo_id, &coin).unwrap();

        // create an input referring to a coin that is already spent
        let input = Input::coin_signed(
            spent_utxo_id,
            owner,
            amount,
            asset_id,
            Default::default(),
            0,
            0,
        );
        let output = Output::Change {
            to: owner,
            amount: 0,
            asset_id,
        };
        let tx = Transaction::script(
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

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let mut block_db_transaction = producer.database.transaction();
        let produce_result = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinAlreadySpent(_)
            ))
        ));

        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinAlreadySpent(_)
            ))
        ));
    }

    // invalidate a block if a tx input doesn't exist
    #[tokio::test]
    async fn executor_invalidates_missing_inputs() {
        // create an input which doesn't exist in the utxo set
        let mut rng = StdRng::seed_from_u64(2322u64);

        let tx = TransactionBuilder::script(
            vec![Opcode::RET(REG_ONE)].into_iter().collect(),
            vec![],
        )
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            10,
            Default::default(),
            Default::default(),
            0,
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

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let mut block_db_transaction = producer.database.transaction();
        let produce_result = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .await;
        assert!(matches!(
            produce_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            ))
        ));

        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .await;
        assert!(matches!(
            verify_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            ))
        ));
    }

    // corrupt a produced block by randomizing change amount
    // and verify that the executor invalidates the tx
    #[tokio::test]
    async fn executor_invalidates_blocks_with_diverging_tx_outputs() {
        let input_amount = 10;
        let fake_output_amount = 100;

        let tx: Transaction = TxBuilder::new(2322u64)
            .gas_limit(1)
            .coin_input(Default::default(), input_amount)
            .change_output(Default::default())
            .build()
            .into();

        let tx_id = tx.id();

        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let mut block = FuelBlock::default();
        *block.transactions_mut() = vec![tx];

        let mut block = producer
            .execute(ExecutionBlock::Production(block.into()))
            .await
            .unwrap();

        // modify change amount
        if let Transaction::Script { outputs, .. } = &mut block.transactions_mut()[0] {
            if let Output::Change { amount, .. } = &mut outputs[0] {
                *amount = fake_output_amount
            }
        }

        let verify_result = verifier.execute(ExecutionBlock::Validation(block)).await;
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
        let tx: Transaction = TxBuilder::new(2322u64)
            .gas_limit(1)
            .coin_input(Default::default(), 10)
            .change_output(Default::default())
            .build()
            .into();

        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

        let mut block = FuelBlock::default();
        *block.transactions_mut() = vec![tx];

        let mut block = producer
            .execute(ExecutionBlock::Production(block.into()))
            .await
            .unwrap();

        // randomize transaction commitment
        block.header_mut().application.generated.transactions_root = rng.gen();

        let verify_result = verifier.execute(ExecutionBlock::Validation(block)).await;

        assert!(matches!(verify_result, Err(Error::InvalidTransactionRoot)))
    }

    // invalidate a block if a tx is missing at least one coin input
    #[tokio::test]
    async fn executor_invalidates_missing_coin_input() {
        let tx: Transaction = TxBuilder::new(2322u64).build().into();
        let tx_id = tx.id();

        let executor = Executor {
            database: Database::default(),
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let err = executor
            .execute(ExecutionBlock::Production(block))
            .await
            .err()
            .unwrap();

        // assert block failed to validate when transaction didn't contain any coin inputs
        assert!(matches!(
            err,
            Error::TransactionValidity(TransactionValidityError::NoCoinOrMessageInput(id)) if id == tx_id
        ));
    }

    #[tokio::test]
    async fn input_coins_are_marked_as_spent() {
        // ensure coins are marked as spent after tx is processed
        let tx: Transaction = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .into();

        let db = &Database::default();
        let executor = Executor {
            database: db.clone(),
            config: Config::local_node(),
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let block = executor
            .execute(ExecutionBlock::Production(block))
            .await
            .unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(block.transactions()[0].inputs()[0].utxo_id().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Spent);
    }

    #[tokio::test]
    async fn input_coins_are_marked_as_spent_with_utxo_validation_enabled() {
        // ensure coins are marked as spent after tx is processed
        let mut rng = StdRng::seed_from_u64(2322u64);
        let starting_block = BlockHeight::from(5u64);

        let tx = TransactionBuilder::script(
            vec![Opcode::RET(REG_ONE)].into_iter().collect(),
            vec![],
        )
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            100,
            Default::default(),
            Default::default(),
            0,
        )
        .add_output(Output::Change {
            to: Default::default(),
            amount: 0,
            asset_id: Default::default(),
        })
        .finalize();
        let db = &mut Database::default();

        // insert coin into state
        if let Input::CoinSigned {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        }
        | Input::CoinPredicate {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        } = tx.inputs()[0]
        {
            db.storage::<Coins>()
                .insert(
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

        let block = PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height: 6u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx],
        };

        let block = executor
            .execute(ExecutionBlock::Production(block))
            .await
            .unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(block.transactions()[0].inputs()[0].utxo_id().unwrap())
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
        let first_block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let tx2: Transaction = TxBuilder::new(2322)
            .start_script(vec![Opcode::RET(1)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .into();

        let second_block = PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height: 2u64.into(),
                    ..Default::default()
                },
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
            .execute(ExecutionBlock::Production(first_block))
            .await
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor {
            database: producer_view,
            config: Config::local_node(),
        };
        let second_block = producer
            .execute(ExecutionBlock::Production(second_block))
            .await
            .unwrap();

        let verifier = Executor {
            database: db,
            config: Config::local_node(),
        };
        let verify_result = verifier
            .execute(ExecutionBlock::Validation(second_block))
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
        let tx2: Transaction = TxBuilder::new(2322)
            .start_script(
                vec![Opcode::ADDI(0x10, REG_ZERO, 0), Opcode::RET(1)],
                vec![],
            )
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .into();

        let first_block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx, tx2],
        };

        let tx3: Transaction = TxBuilder::new(2322)
            .start_script(
                vec![Opcode::ADDI(0x10, REG_ZERO, 1), Opcode::RET(1)],
                vec![],
            )
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .into();
        let tx_id = tx3.id();

        let second_block = PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height: 2u64.into(),
                    ..Default::default()
                },
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
            .execute(ExecutionBlock::Production(first_block))
            .await
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor {
            database: producer_view,
            config: Config::local_node(),
        };

        let mut second_block = producer
            .execute(ExecutionBlock::Production(second_block))
            .await
            .unwrap();
        // Corrupt the utxo_id of the contract output
        if let Transaction::Script { inputs, .. } =
            &mut second_block.transactions_mut()[0]
        {
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
            .execute(ExecutionBlock::Validation(second_block))
            .await;

        assert!(matches!(
            verify_result,
            Err(Error::InvalidTransactionOutcome {
                transaction_id
            }) if transaction_id == tx_id
        ));
    }

    #[tokio::test]
    async fn outputs_with_amount_are_included_utxo_set() {
        let mut rng = StdRng::seed_from_u64(2322);
        let asset_id: AssetId = rng.gen();
        let owner: Address = rng.gen();
        let input_amount = 1000;
        let variable_transfer_amount = 100;
        let coin_output_amount = 150;

        let (tx, contract_id) = create_contract(
            vec![
                // load amount of coins to 0x10
                Opcode::ADDI(0x10, REG_FP, CallFrame::a_offset() as Immediate12),
                Opcode::LW(0x10, 0x10, 0),
                // load asset id to 0x11
                Opcode::ADDI(0x11, REG_FP, CallFrame::b_offset() as Immediate12),
                Opcode::LW(0x11, 0x11, 0),
                // load address to 0x12
                Opcode::ADDI(0x12, 0x11, 32),
                // load output index (0) to 0x13
                Opcode::ADDI(0x13, REG_ZERO, 0),
                Opcode::TRO(0x12, 0x13, 0x10, 0x11),
                Opcode::RET(REG_ONE),
            ]
            .into_iter()
            .collect::<Vec<u8>>(),
            &mut rng,
        );
        let (script, data_offset) = script_with_data_offset!(
            data_offset,
            vec![
                // set reg 0x10 to call data
                Opcode::MOVI(0x10, (data_offset + 64) as Immediate18),
                // set reg 0x11 to asset id
                Opcode::MOVI(0x11, data_offset),
                // set reg 0x12 to call amount
                Opcode::MOVI(0x12, variable_transfer_amount),
                // call contract without any tokens to transfer in (3rd arg arbitrary when 2nd is zero)
                Opcode::CALL(0x10, 0x12, 0x11, REG_CGAS),
                Opcode::RET(REG_ONE),
            ],
            ConsensusParameters::DEFAULT.tx_offset()
        );

        let script_data: Vec<u8> = [
            asset_id.as_ref(),
            owner.as_ref(),
            Call::new(
                contract_id,
                variable_transfer_amount as Word,
                data_offset as Word,
            )
            .to_bytes()
            .as_ref(),
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect();

        let tx2: Transaction = TxBuilder::new(2322)
            .gas_limit(ConsensusParameters::DEFAULT.max_gas_per_tx)
            .start_script(script, script_data)
            .contract_input(contract_id)
            .coin_input(asset_id, input_amount)
            .variable_output(Default::default())
            .coin_output(asset_id, coin_output_amount)
            .change_output(asset_id)
            .contract_output(&contract_id)
            .build()
            .into();
        let tx2_id = tx2.id();

        let database = &Database::default();
        let executor = Executor {
            database: database.clone(),
            config: Config::local_node(),
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx, tx2],
        };

        let block = executor
            .execute(ExecutionBlock::Production(block))
            .await
            .unwrap();

        // ensure that all utxos with an amount are stored into the utxo set
        for (idx, output) in block.transactions()[1].outputs().iter().enumerate() {
            let id = fuel_tx::UtxoId::new(tx2_id, idx as u8);
            match output {
                Output::Change { .. } | Output::Variable { .. } | Output::Coin { .. } => {
                    let maybe_utxo = database.storage::<Coins>().get(&id).unwrap();
                    assert!(maybe_utxo.is_some());
                    let utxo = maybe_utxo.unwrap();
                    assert!(utxo.amount > 0)
                }
                _ => (),
            }
        }
    }

    #[tokio::test]
    async fn outputs_with_no_value_are_excluded_from_utxo_set() {
        let mut rng = StdRng::seed_from_u64(2322);
        let asset_id: AssetId = rng.gen();
        let input_amount = 0;
        let coin_output_amount = 0;

        let tx: Transaction = TxBuilder::new(2322)
            .coin_input(asset_id, input_amount)
            .variable_output(Default::default())
            .coin_output(asset_id, coin_output_amount)
            .change_output(asset_id)
            .build()
            .into();
        let tx_id = tx.id();

        let database = &Database::default();
        let executor = Executor {
            database: database.clone(),
            config: Config::local_node(),
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        executor
            .execute(ExecutionBlock::Production(block))
            .await
            .unwrap();

        for idx in 0..2 {
            let id = UtxoId::new(tx_id, idx);
            let maybe_utxo = database.storage::<Coins>().get(&id).unwrap();
            assert!(maybe_utxo.is_none());
        }
    }

    /// Helper to build transactions and a message in it for some of the message tests
    fn make_tx_and_message(
        rng: &mut StdRng,
        da_height: u64,
    ) -> (Transaction, CheckedMessage) {
        let mut message = Message {
            sender: rng.gen(),
            recipient: rng.gen(),
            nonce: rng.gen(),
            amount: 1000,
            data: vec![],
            da_height: DaBlockHeight(da_height),
            fuel_block_spend: None,
        };

        let tx = TransactionBuilder::script(vec![], vec![])
            .add_unsigned_message_input(
                rng.gen(),
                message.sender,
                message.nonce,
                message.amount,
                vec![],
            )
            .finalize();

        if let Input::MessageSigned { recipient, .. } = tx.inputs()[0] {
            message.recipient = recipient;
        } else {
            unreachable!();
        }

        (tx, message.check())
    }

    /// Helper to build database and executor for some of the message tests
    async fn make_executor(messages: &[&CheckedMessage]) -> Executor {
        let mut database = Database::default();

        for message in messages {
            database.insert_message(message).await;
        }

        Executor {
            database,
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        }
    }

    #[tokio::test]
    async fn unspent_message_succeeds_when_msg_da_height_lt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 0);

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let block = make_executor(&[&message])
            .await
            .execute(ExecutionBlock::Production(block))
            .await
            .expect("block execution failed unexpectedly");

        make_executor(&[&message])
            .await
            .execute(ExecutionBlock::Validation(block))
            .await
            .expect("block validation failed unexpectedly");
    }

    #[tokio::test]
    async fn message_fails_when_spending_nonexistent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, _message) = make_tx_and_message(&mut rng, 0);

        let mut block = FuelBlock::default();
        *block.transactions_mut() = vec![tx.clone()];

        let res = make_executor(&[]) // No messages in the db
            .await
            .execute(ExecutionBlock::Production(block.clone().into()))
            .await;
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            ))
        ));

        let res = make_executor(&[]) // No messages in the db
            .await
            .execute(ExecutionBlock::Validation(block))
            .await;
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            ))
        ));
    }

    #[tokio::test]
    async fn message_fails_when_spending_da_height_gt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 1); // Block has zero da_height

        let mut block = FuelBlock::default();
        *block.transactions_mut() = vec![tx];

        let res = make_executor(&[&message])
            .await
            .execute(ExecutionBlock::Production(block.clone().into()))
            .await;
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageSpendTooEarly(_)
            ))
        ));

        let res = make_executor(&[&message])
            .await
            .execute(ExecutionBlock::Validation(block))
            .await;
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageSpendTooEarly(_)
            ))
        ));
    }

    #[tokio::test]
    async fn message_fails_when_spending_already_spent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        // Create two transactions with the same message
        let (tx1, message) = make_tx_and_message(&mut rng, 0);
        let (mut tx2, _) = make_tx_and_message(&mut rng, 0);
        tx2.inputs_mut()[0] = tx1.inputs()[0].clone();

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1, tx2],
        };

        let exec = make_executor(&[&message]).await;
        let mut block_db_transaction = exec.database.transaction();
        let res = exec
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .await;
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            ))
        ));

        let exec = make_executor(&[&message]).await;
        let mut block_db_transaction = exec.database.transaction();
        let res = exec
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .await;
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            ))
        ));
    }

    #[tokio::test]
    async fn get_block_height_returns_current_executing_block() {
        let mut rng = StdRng::seed_from_u64(1234);

        // return current block height
        let script = vec![Opcode::BHEI(0x10), Opcode::RET(0x10)];
        let tx = TransactionBuilder::script(script.into_iter().collect(), vec![])
            .gas_limit(10000)
            .add_unsigned_coin_input(
                rng.gen(),
                rng.gen(),
                1000,
                AssetId::zeroed(),
                Default::default(),
                0,
            )
            .finalize();

        // setup block
        let block_height = rng.gen_range(5u32..1000u32);

        let block = PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height: block_height.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.clone()],
        };

        // setup db with coin to spend
        let database = &mut &mut Database::default();
        let coin_input = &tx.inputs()[0];
        database
            .storage::<Coins>()
            .insert(
                coin_input.utxo_id().unwrap(),
                &Coin {
                    owner: *coin_input.input_owner().unwrap(),
                    amount: coin_input.amount().unwrap(),
                    asset_id: *coin_input.asset_id().unwrap(),
                    maturity: (coin_input.maturity().unwrap()).into(),
                    block_created: 0u64.into(),
                    status: CoinStatus::Unspent,
                },
            )
            .unwrap();

        // make executor with db
        let executor = Executor {
            database: database.clone(),
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        };

        executor
            .execute(ExecutionBlock::Production(block))
            .await
            .unwrap();

        let receipts = database
            .storage::<Receipts>()
            .get(&tx.id())
            .unwrap()
            .unwrap();
        assert_eq!(block_height as u64, receipts[0].val().unwrap());
    }

    #[tokio::test]
    async fn get_time_returns_current_executing_block_time() {
        let mut rng = StdRng::seed_from_u64(1234);

        // return current block height
        let script = vec![
            Opcode::BHEI(0x10),
            Opcode::TIME(0x11, 0x10),
            Opcode::RET(0x11),
        ];
        let tx = TransactionBuilder::script(script.into_iter().collect(), vec![])
            .gas_limit(10000)
            .add_unsigned_coin_input(
                rng.gen(),
                rng.gen(),
                1000,
                AssetId::zeroed(),
                Default::default(),
                0,
            )
            .finalize();

        // setup block
        let block_height = rng.gen_range(5u32..1000u32);
        let time = Utc.timestamp(rng.gen_range(1u32..u32::MAX) as i64, 0);

        let block = PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
                    height: block_height.into(),
                    time,
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.clone()],
        };

        // setup db with coin to spend
        let database = &mut &mut Database::default();
        let coin_input = &tx.inputs()[0];
        database
            .storage::<Coins>()
            .insert(
                coin_input.utxo_id().unwrap(),
                &Coin {
                    owner: *coin_input.input_owner().unwrap(),
                    amount: coin_input.amount().unwrap(),
                    asset_id: *coin_input.asset_id().unwrap(),
                    maturity: (coin_input.maturity().unwrap()).into(),
                    block_created: 0u64.into(),
                    status: CoinStatus::Unspent,
                },
            )
            .unwrap();

        // make executor with db
        let executor = Executor {
            database: database.clone(),
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        };

        executor
            .execute(ExecutionBlock::Production(block))
            .await
            .unwrap();

        let receipts = database
            .storage::<Receipts>()
            .get(&tx.id())
            .unwrap()
            .unwrap();

        assert_eq!(time.timestamp_millis() as Word, receipts[0].val().unwrap());
    }
}
