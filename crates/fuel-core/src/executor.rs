use crate::{
    database::{
        transaction::DatabaseTransaction,
        transactions::TransactionIndex,
        vm_database::VmDatabase,
        Database,
    },
    service::Config,
};
use fuel_core_executor::refs::ContractRef;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsLatestUtxo,
        FuelBlocks,
        Messages,
        Receipts,
        SpentMessages,
        Transactions,
    },
    transactional::{
        StorageTransaction,
        Transaction as StorageTransactionTrait,
    },
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        header::PartialBlockHeader,
        primitives::{
            BlockHeight,
            DaBlockHeight,
        },
    },
    entities::{
        coins::coin::CompressedCoin,
        contract::ContractUtxoInfo,
    },
    fuel_asm::{
        RegId,
        Word,
    },
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
            TxPointer as TxPointerField,
        },
        input::{
            coin::{
                CoinPredicate,
                CoinSigned,
            },
            contract::Contract,
            message::{
                MessageCoinPredicate,
                MessageCoinSigned,
                MessageDataPredicate,
                MessageDataSigned,
            },
        },
        Address,
        AssetId,
        Bytes32,
        Input,
        Mint,
        Output,
        Receipt,
        Transaction,
        TransactionFee,
        TxPointer,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::MessageId,
    fuel_vm::{
        checked_transaction::{
            Checked,
            CreateCheckedMetadata,
            IntoChecked,
            ScriptCheckedMetadata,
        },
        interpreter::{
            CheckedMetadata,
            ExecutableTransaction,
        },
        state::StateTransition,
        Backtrace as FuelBacktrace,
        Interpreter,
        PredicateStorage,
    },
    services::{
        executor::{
            Error as ExecutorError,
            ExecutionBlock,
            ExecutionKind,
            ExecutionResult,
            ExecutionType,
            ExecutionTypes,
            Result as ExecutorResult,
            TransactionExecutionResult,
            TransactionExecutionStatus,
            TransactionValidityError,
            UncommittedResult,
        },
        txpool::TransactionStatus,
    },
};
use itertools::Itertools;
pub use ports::RelayerPort;
use std::{
    borrow::Cow,
    ops::{
        Deref,
        DerefMut,
    },
};
use tracing::{
    debug,
    warn,
};

mod ports;

/// ! The executor is used for block production and validation. Given a block, it will execute all
/// the transactions contained in the block and persist changes to the underlying database as needed.
/// In production mode, block fields like transaction commitments are set based on the executed txs.
/// In validation mode, the processed block commitments are compared with the proposed block.
#[derive(Clone, Debug)]
pub struct Executor<R>
where
    R: RelayerPort + Clone,
{
    pub database: Database,
    pub relayer: R,
    pub config: Config,
}

/// Data that is generated after executing all transactions.
struct ExecutionData {
    coinbase: u64,
    message_ids: Vec<MessageId>,
    tx_status: Vec<TransactionExecutionStatus>,
    skipped_transactions: Vec<(Transaction, ExecutorError)>,
}

impl<R> Executor<R>
where
    R: RelayerPort + Clone,
{
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn execute_and_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<ExecutionResult> {
        let (result, db_transaction) = self.execute_without_commit(block)?.into();
        db_transaction.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
impl Executor<Database> {
    fn test(database: Database, config: Config) -> Self {
        Self {
            relayer: database.clone(),
            database,
            config,
        }
    }
}

impl<R> Executor<R>
where
    R: RelayerPort + Clone,
{
    pub fn execute_without_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        self.execute_inner(block, &self.database)
    }

    pub fn dry_run(
        &self,
        block: ExecutionBlock,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<Vec<Receipt>>> {
        let database = self.database.clone();

        // fallback to service config value if no utxo_validation override is provided
        let utxo_validation = utxo_validation.unwrap_or(self.config.utxo_validation);

        // spawn a nested executor instance to override utxo_validation config
        let executor = Self {
            relayer: self.relayer.clone(),
            config: Config {
                utxo_validation,
                ..self.config.clone()
            },
            database,
        };

        let (
            ExecutionResult {
                block,
                skipped_transactions,
                ..
            },
            temporary_db,
        ) = executor.execute_without_commit(block)?.into();

        // If one of the transactions fails, return an error.
        if let Some((_, err)) = skipped_transactions.into_iter().next() {
            return Err(err)
        }

        block
            .transactions()
            .iter()
            .map(|tx| {
                let id = tx.id();
                StorageInspect::<Receipts>::get(temporary_db.as_ref(), &id)
                    .transpose()
                    .unwrap_or_else(|| Ok(Default::default()))
                    .map(|v| v.into_owned())
            })
            .collect::<Result<Vec<Vec<Receipt>>, _>>()
            .map_err(Into::into)
        // drop `temporary_db` without committing to avoid altering state.
    }
}

impl<R> Executor<R>
where
    R: RelayerPort + Clone,
{
    #[tracing::instrument(skip(self))]
    fn execute_inner(
        &self,
        block: ExecutionBlock,
        database: &Database,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>> {
        // Compute the block id before execution if there is one.
        let pre_exec_block_id = block.id();

        // If there is full fuel block for validation then map it into
        // a partial header.
        let mut block = block.map_v(PartialFuelBlock::from);

        // Create a new database transaction.
        let mut block_db_transaction = database.transaction();

        // Execute all transactions.
        let execution_data =
            self.execute_transactions(&mut block_db_transaction, block.as_mut())?;

        let ExecutionData {
            coinbase,
            message_ids,
            tx_status,
            skipped_transactions,
        } = execution_data;

        // Now that the transactions have been executed, generate the full header.
        let block = block
            .map(|b: PartialFuelBlock| b.generate(&message_ids[..]))
            .into_inner();

        let finalized_block_id = block.id();

        debug!(
            "Block {:#x} fees: {}",
            pre_exec_block_id.unwrap_or(finalized_block_id),
            coinbase
        );

        // check if block id doesn't match proposed block id
        if let Some(pre_exec_block_id) = pre_exec_block_id {
            // The block id comparison compares the whole blocks including all fields.
            if pre_exec_block_id != finalized_block_id {
                return Err(ExecutorError::InvalidBlockId)
            }
        }

        let result = ExecutionResult {
            block,
            skipped_transactions,
            tx_status,
        };

        // ------------ GraphQL API Functionality BEGIN ------------

        // save the status for every transaction using the finalized block id
        self.persist_transaction_status(&result, block_db_transaction.deref_mut())?;

        // save the associated owner for each transaction in the block
        self.index_tx_owners_for_block(&result.block, &mut block_db_transaction)?;

        // ------------ GraphQL API Functionality   END ------------

        // insert block into database
        block_db_transaction
            .deref_mut()
            .storage::<FuelBlocks>()
            .insert(&finalized_block_id, &result.block.compress())?;

        // Get the complete fuel block.
        Ok(UncommittedResult::new(
            result,
            StorageTransaction::new(block_db_transaction),
        ))
    }

    #[tracing::instrument(skip(self))]
    /// Execute all transactions on the fuel block.
    fn execute_transactions(
        &self,
        block_db_transaction: &mut DatabaseTransaction,
        block: ExecutionType<&mut PartialFuelBlock>,
    ) -> ExecutorResult<ExecutionData> {
        let mut data = ExecutionData {
            coinbase: 0,
            message_ids: Vec::new(),
            tx_status: Vec::new(),
            skipped_transactions: Vec::new(),
        };
        let execution_data = &mut data;

        // Split out the execution kind and partial block.
        let (execution_kind, block) = block.split();

        let block_height: u32 = (*block.header.height()).into();

        // Clean block from transactions and gather them from scratch.
        let mut iter = ::core::mem::take(&mut block.transactions).into_iter();

        let mut coinbase_tx: Mint = match execution_kind {
            ExecutionKind::Production => {
                // The coinbase transaction should be the first.
                // We will add actual amount of `Output::Coin` at the end of transactions execution.
                Transaction::mint(
                    TxPointer::new(block_height, 0),
                    vec![Output::coin(
                        self.config.block_producer.coinbase_recipient,
                        0, // We will set it later
                        AssetId::BASE,
                    )],
                )
            }
            ExecutionKind::Validation => {
                let mint = if let Some(Transaction::Mint(mint)) = iter.next() {
                    mint
                } else {
                    return Err(ExecutorError::CoinbaseIsNotFirstTransaction)
                };
                self.check_coinbase(block_height as Word, mint, None)?
            }
        };

        // Skip the coinbase transaction.
        block.transactions.push(coinbase_tx.clone().into());
        let mut tx_index = 1;

        let mut filtered_transactions: Vec<_> = iter
            .filter_map(|transaction| {
                let mut filter_tx = |mut tx, idx| {
                    let mut tx_db_transaction = block_db_transaction.transaction();
                    let result = self.execute_transaction(
                        idx,
                        &mut tx,
                        &block.header,
                        execution_data,
                        execution_kind,
                        &mut tx_db_transaction,
                    );

                    if let Err(err) = result {
                        return match execution_kind {
                            ExecutionKind::Production => {
                                // If, during block production, we get an invalid transaction,
                                // remove it from the block and continue block creation. An invalid
                                // transaction means that the caller didn't validate it first, so
                                // maybe something is wrong with validation rules in the `TxPool`
                                // (or in another place that should validate it). Or we forgot to
                                // clean up some dependent/conflict transactions. But it definitely
                                // means that something went wrong, and we must fix it.
                                execution_data.skipped_transactions.push((tx, err));
                                None
                            }
                            ExecutionKind::Validation => Some(Err(err)),
                        }
                    }

                    if let Err(err) = tx_db_transaction.commit() {
                        return Some(Err(err.into()))
                    }
                    Some(Ok(tx))
                };

                let filtered_tx = filter_tx(transaction, tx_index);
                if filtered_tx.is_some() {
                    tx_index += 1;
                }
                filtered_tx
            })
            .try_collect()?;

        // After the execution of all transactions in production mode, we can set the final fee.
        if let ExecutionKind::Production = execution_kind {
            coinbase_tx.outputs_mut().clear();
            coinbase_tx.outputs_mut().push(Output::coin(
                self.config.block_producer.coinbase_recipient,
                execution_data.coinbase,
                AssetId::BASE,
            ));
            block.transactions[0] = coinbase_tx.clone().into();
        }
        block.transactions.append(&mut filtered_transactions);

        coinbase_tx = self.check_coinbase(
            block_height as Word,
            coinbase_tx,
            Some(execution_data.coinbase),
        )?;
        self.apply_coinbase(coinbase_tx, block, execution_data, block_db_transaction)?;

        Ok(data)
    }

    fn execute_transaction(
        &self,
        idx: u16,
        tx: &mut Transaction,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
        execution_kind: ExecutionKind,
        tx_db_transaction: &mut DatabaseTransaction,
    ) -> ExecutorResult<()> {
        let tx_id = tx.id();
        // Throw a clear error if the transaction id is a duplicate
        if tx_db_transaction
            .deref_mut()
            .storage::<Transactions>()
            .contains_key(&tx_id)?
        {
            return Err(ExecutorError::TransactionIdCollision(tx_id))
        }

        match tx {
            Transaction::Script(script) => self.execute_create_or_script(
                idx,
                script,
                header,
                execution_data,
                tx_db_transaction,
                execution_kind,
            )?,
            Transaction::Create(create) => self.execute_create_or_script(
                idx,
                create,
                header,
                execution_data,
                tx_db_transaction,
                execution_kind,
            )?,
            Transaction::Mint(mint) => {
                // Right now, we only support `Mint` transactions for coinbase,
                // which are processed separately as a first transaction.
                //
                // All other `Mint` transactions are not allowed.
                return Err(ExecutorError::NotSupportedTransaction(Box::new(
                    mint.clone().into(),
                )))
            }
        };

        Ok(())
    }

    fn apply_coinbase(
        &self,
        coinbase_tx: Mint,
        block: &PartialFuelBlock,
        execution_data: &mut ExecutionData,
        block_db_transaction: &mut DatabaseTransaction,
    ) -> ExecutorResult<()> {
        let block_height = *block.header.height();
        let coinbase_id = coinbase_tx.id();
        self.persist_output_utxos(
            block_height,
            0,
            &coinbase_id,
            block_db_transaction,
            &[],
            coinbase_tx.outputs(),
        )?;
        execution_data.tx_status.insert(
            0,
            TransactionExecutionStatus {
                id: coinbase_id,
                result: TransactionExecutionResult::Success { result: None },
            },
        );
        if block_db_transaction
            .deref_mut()
            .storage::<Transactions>()
            .insert(&coinbase_id, &coinbase_tx.into())?
            .is_some()
        {
            return Err(ExecutorError::TransactionIdCollision(coinbase_id))
        }
        Ok(())
    }

    fn check_coinbase(
        &self,
        block_height: Word,
        mint: Mint,
        expected_amount: Option<Word>,
    ) -> ExecutorResult<Mint> {
        let checked_mint = mint.into_checked(
            block_height,
            &self.config.chain_conf.transaction_parameters,
            &self.config.chain_conf.gas_costs,
        )?;

        if checked_mint.transaction().tx_pointer().tx_index() != 0 {
            return Err(ExecutorError::CoinbaseIsNotFirstTransaction)
        }

        if checked_mint.transaction().outputs().len() > 1 {
            return Err(ExecutorError::CoinbaseSeveralOutputs)
        }

        if let Some(Output::Coin {
            asset_id, amount, ..
        }) = checked_mint.transaction().outputs().first()
        {
            if asset_id != &AssetId::BASE {
                return Err(ExecutorError::CoinbaseOutputIsInvalid)
            }

            if let Some(expected_amount) = expected_amount {
                if expected_amount != *amount {
                    return Err(ExecutorError::CoinbaseAmountMismatch)
                }
            }
        } else {
            return Err(ExecutorError::CoinbaseOutputIsInvalid)
        }

        let (mint, _) = checked_mint.into();
        Ok(mint)
    }

    fn execute_create_or_script<Tx>(
        &self,
        idx: u16,
        original_tx: &mut Tx,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
        tx_db_transaction: &mut DatabaseTransaction,
        execution_kind: ExecutionKind,
    ) -> ExecutorResult<()>
    where
        Tx: ExecutableTransaction + PartialEq,
        <Tx as IntoChecked>::Metadata: Fee + CheckedMetadata + Clone,
    {
        let block_height: u32 = (*header.height()).into();
        // Wrap the transaction in the execution kind.
        self.compute_inputs(
            match execution_kind {
                ExecutionKind::Production => ExecutionTypes::Production(original_tx),
                ExecutionKind::Validation => ExecutionTypes::Validation(original_tx),
            },
            tx_db_transaction.deref_mut(),
        )?;

        let checked_tx = original_tx.clone().into_checked_basic(
            block_height as Word,
            &self.config.chain_conf.transaction_parameters,
        )?;

        let tx_id = checked_tx.transaction().id();
        let min_fee = checked_tx.metadata().min_fee();
        let max_fee = checked_tx.metadata().max_fee();

        self.verify_tx_predicates(checked_tx.clone())?;

        if self.config.utxo_validation {
            // validate transaction has at least one coin
            self.verify_tx_has_at_least_one_coin_or_message(checked_tx.transaction())?;
            // validate utxos exist and maturity is properly set
            self.verify_input_state(
                tx_db_transaction.deref(),
                checked_tx.transaction(),
                *header.height(),
                header.da_height,
            )?;
            // validate transaction signature
            checked_tx
                .transaction()
                .check_signatures()
                .map_err(TransactionValidityError::from)?;
        }

        // execute transaction
        // setup database view that only lives for the duration of vm execution
        let mut sub_block_db_commit = tx_db_transaction.transaction();
        let sub_db_view = sub_block_db_commit.as_mut();
        // execution vm
        let vm_db = VmDatabase::new(
            sub_db_view.clone(),
            &header.consensus,
            self.config.block_producer.coinbase_recipient,
        );
        let mut vm = Interpreter::with_storage(
            vm_db,
            self.config.chain_conf.transaction_parameters,
            self.config.chain_conf.gas_costs.clone(),
        );
        let vm_result: StateTransition<_> = vm
            .transact(checked_tx.clone())
            .map_err(|error| ExecutorError::VmExecution {
                error,
                transaction_id: tx_id,
            })?
            .into();
        let reverted = vm_result.should_revert();

        // TODO: Avoid cloning here, we can extract value from result
        let mut tx = vm_result.tx().clone();
        // only commit state changes if execution was a success
        if !reverted {
            sub_block_db_commit.commit()?;
        }

        // update block commitment
        let tx_fee =
            self.total_fee_paid(min_fee, max_fee, tx.price(), vm_result.receipts())?;

        // Check or set the executed transaction.
        match execution_kind {
            ExecutionKind::Validation => {
                // ensure tx matches vm output exactly
                if &tx != checked_tx.transaction() {
                    return Err(ExecutorError::InvalidTransactionOutcome {
                        transaction_id: tx_id,
                    })
                }
            }
            ExecutionKind::Production => {
                // malleate the block with the resultant tx from the vm
            }
        }

        // change the spent status of the tx inputs
        self.spend_input_utxos(&tx, tx_db_transaction.deref_mut(), reverted)?;

        // Persist utxos first and after calculate the not utxo outputs
        self.persist_output_utxos(
            *header.height(),
            idx,
            &tx_id,
            tx_db_transaction.deref_mut(),
            tx.inputs(),
            tx.outputs(),
        )?;
        self.compute_not_utxo_outputs(
            match execution_kind {
                ExecutionKind::Production => ExecutionTypes::Production(&mut tx),
                ExecutionKind::Validation => ExecutionTypes::Validation(&tx),
            },
            tx_db_transaction.deref_mut(),
        )?;

        *original_tx = tx;

        // Store tx into the block db transaction
        tx_db_transaction
            .deref_mut()
            .storage::<Transactions>()
            // TODO: Avoid cloning here
            .insert(&tx_id, &original_tx.clone().into())?;

        // persist receipts
        self.persist_receipts(
            &tx_id,
            vm_result.receipts(),
            tx_db_transaction.deref_mut(),
        )?;

        let status = if vm_result.should_revert() {
            self.log_backtrace(&vm, vm_result.receipts());
            // get reason for revert
            let reason = vm_result
                .receipts()
                .iter()
                .find_map(|receipt| match receipt {
                    // Format as `Revert($rA)`
                    Receipt::Revert { ra, .. } => Some(format!("Revert({ra})")),
                    // Display PanicReason e.g. `OutOfGas`
                    Receipt::Panic { reason, .. } => Some(format!("{}", reason.reason())),
                    _ => None,
                })
                .unwrap_or_else(|| format!("{:?}", vm_result.state()));

            TransactionExecutionResult::Failed {
                reason,
                result: Some(*vm_result.state()),
            }
        } else {
            // else tx was a success
            TransactionExecutionResult::Success {
                result: Some(*vm_result.state()),
            }
        };

        // Update `execution_data` data only after all steps.
        execution_data.coinbase = execution_data
            .coinbase
            .checked_add(tx_fee)
            .ok_or(ExecutorError::FeeOverflow)?;
        // queue up status for this tx to be stored once block id is finalized.
        execution_data.tx_status.push(TransactionExecutionStatus {
            id: tx_id,
            result: status,
        });
        execution_data
            .message_ids
            .extend(vm_result.receipts().iter().filter_map(|r| r.message_id()));

        Ok(())
    }

    fn verify_input_state<Tx: ExecutableTransaction>(
        &self,
        db: &Database,
        transaction: &Tx,
        block_height: BlockHeight,
        block_da_height: DaBlockHeight,
    ) -> ExecutorResult<()> {
        for input in transaction.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // TODO: Check that fields are equal. We already do that check
                    //  in the `fuel-core-txpool`, so we need to reuse the code here.
                    if let Some(coin) = db.storage::<Coins>().get(utxo_id)? {
                        if block_height
                            < BlockHeight::from(coin.tx_pointer.block_height())
                                + coin.maturity
                        {
                            return Err(TransactionValidityError::CoinHasNotMatured(
                                *utxo_id,
                            )
                            .into())
                        }
                    } else {
                        return Err(
                            TransactionValidityError::CoinDoesNotExist(*utxo_id).into()
                        )
                    }
                }
                Input::Contract(_) => {}
                Input::MessageCoinSigned(MessageCoinSigned {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    ..
                })
                | Input::MessageCoinPredicate(MessageCoinPredicate {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    ..
                })
                | Input::MessageDataSigned(MessageDataSigned {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    ..
                })
                | Input::MessageDataPredicate(MessageDataPredicate {
                    sender,
                    recipient,
                    amount,
                    nonce,
                    ..
                }) => {
                    // Eagerly return already spent if status is known.
                    if db.is_message_spent(nonce)? {
                        return Err(
                            TransactionValidityError::MessageAlreadySpent(*nonce).into()
                        )
                    }
                    if let Some(message) = self
                        .relayer
                        .get_message(nonce, &block_da_height)
                        .map_err(|e| ExecutorError::RelayerError(e.into()))?
                    {
                        if message.da_height > block_da_height {
                            return Err(TransactionValidityError::MessageSpendTooEarly(
                                *nonce,
                            )
                            .into())
                        }
                        if message.sender != *sender {
                            return Err(TransactionValidityError::MessageSenderMismatch(
                                *nonce,
                            )
                            .into())
                        }
                        if message.recipient != *recipient {
                            return Err(
                                TransactionValidityError::MessageRecipientMismatch(
                                    *nonce,
                                )
                                .into(),
                            )
                        }
                        if message.amount != *amount {
                            return Err(TransactionValidityError::MessageAmountMismatch(
                                *nonce,
                            )
                            .into())
                        }
                        if message.nonce != *nonce {
                            return Err(TransactionValidityError::MessageNonceMismatch(
                                *nonce,
                            )
                            .into())
                        }
                        let expected_data = if message.data.is_empty() {
                            None
                        } else {
                            Some(message.data.as_slice())
                        };
                        if expected_data != input.input_data() {
                            return Err(TransactionValidityError::MessageDataMismatch(
                                *nonce,
                            )
                            .into())
                        }
                    } else {
                        return Err(
                            TransactionValidityError::MessageDoesNotExist(*nonce).into()
                        )
                    }
                }
            }
        }

        Ok(())
    }

    /// Verify all the predicates of a tx.
    pub fn verify_tx_predicates<Tx>(&self, tx: Checked<Tx>) -> ExecutorResult<()>
    where
        Tx: ExecutableTransaction,
        <Tx as IntoChecked>::Metadata: CheckedMetadata,
    {
        let id = tx.transaction().id();
        if Interpreter::<PredicateStorage>::check_predicates(
            tx,
            self.config.chain_conf.transaction_parameters,
            self.config.chain_conf.gas_costs.clone(),
        )
        .is_err()
        {
            return Err(ExecutorError::TransactionValidity(
                TransactionValidityError::InvalidPredicate(id),
            ))
        }

        Ok(())
    }

    /// Verify the transaction has at least one coin.
    ///
    /// TODO: This verification really belongs in fuel-tx, and can be removed once
    ///       https://github.com/FuelLabs/fuel-tx/issues/118 is resolved.
    fn verify_tx_has_at_least_one_coin_or_message<Tx: ExecutableTransaction>(
        &self,
        tx: &Tx,
    ) -> ExecutorResult<()> {
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

    /// Mark input utxos as spent
    fn spend_input_utxos<Tx>(
        &self,
        tx: &Tx,
        db: &mut Database,
        reverted: bool,
    ) -> ExecutorResult<()>
    where
        Tx: ExecutableTransaction,
    {
        for input in tx.inputs() {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    // prune utxo from db
                    db.storage::<Coins>().remove(utxo_id)?;
                }
                Input::MessageDataSigned(_)
                | Input::MessageDataPredicate(_)
                    if reverted => {
                    // Don't spend the retryable messages if transaction is reverted
                    continue
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. }) // Spend only if tx is not reverted
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) // Spend only if tx is not reverted
                 => {
                    // mark message id as spent
                    let was_already_spent =
                        db.storage::<SpentMessages>().insert(nonce, &())?;
                    // ensure message wasn't already marked as spent
                    if was_already_spent.is_some() {
                        return Err(ExecutorError::MessageAlreadySpent(*nonce))
                    }
                    // cleanup message contents
                    db.storage::<Messages>().remove(nonce)?;
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
    ) -> ExecutorResult<Word> {
        for r in receipts {
            if let Receipt::ScriptResult { gas_used, .. } = r {
                return TransactionFee::gas_refund_value(
                    &self.config.chain_conf.transaction_parameters,
                    *gas_used,
                    gas_price,
                )
                .and_then(|refund| max_fee.checked_sub(refund))
                .ok_or(ExecutorError::FeeOverflow)
            }
        }
        // if there's no script result (i.e. create) then fee == base amount
        Ok(min_fee)
    }

    /// Computes all zeroed or variable inputs.
    /// In production mode, updates the inputs with computed values.
    /// In validation mode, compares the inputs with computed inputs.
    fn compute_inputs<Tx>(
        &self,
        tx: ExecutionTypes<&mut Tx, &Tx>,
        db: &mut Database,
    ) -> ExecutorResult<()>
    where
        Tx: ExecutableTransaction,
    {
        match tx {
            ExecutionTypes::Production(tx) => {
                for input in tx.inputs_mut() {
                    match input {
                        Input::CoinSigned(CoinSigned {
                            tx_pointer,
                            utxo_id,
                            owner,
                            amount,
                            asset_id,
                            maturity,
                            ..
                        })
                        | Input::CoinPredicate(CoinPredicate {
                            tx_pointer,
                            utxo_id,
                            owner,
                            amount,
                            asset_id,
                            maturity,
                            ..
                        }) => {
                            let coin = self.get_coin_or_default(
                                db,
                                *utxo_id,
                                *owner,
                                *amount,
                                *asset_id,
                                (*maturity).into(),
                            )?;
                            *tx_pointer = coin.tx_pointer;
                        }
                        Input::Contract(Contract {
                            ref mut utxo_id,
                            ref mut balance_root,
                            ref mut state_root,
                            ref mut tx_pointer,
                            ref contract_id,
                            ..
                        }) => {
                            let mut contract = ContractRef::new(&mut *db, *contract_id);
                            let utxo_info =
                                contract.validated_utxo(self.config.utxo_validation)?;
                            *utxo_id = utxo_info.utxo_id;
                            *tx_pointer = utxo_info.tx_pointer;
                            *balance_root = contract.balance_root()?;
                            *state_root = contract.state_root()?;
                        }
                        _ => {}
                    }
                }
            }
            // Needed to convince the compiler that tx is taken by ref here
            ExecutionTypes::Validation(tx) => {
                for input in tx.inputs() {
                    match input {
                        Input::CoinSigned(CoinSigned {
                            tx_pointer,
                            utxo_id,
                            owner,
                            amount,
                            asset_id,
                            maturity,
                            ..
                        })
                        | Input::CoinPredicate(CoinPredicate {
                            tx_pointer,
                            utxo_id,
                            owner,
                            amount,
                            asset_id,
                            maturity,
                            ..
                        }) => {
                            let coin = self.get_coin_or_default(
                                db,
                                *utxo_id,
                                *owner,
                                *amount,
                                *asset_id,
                                (*maturity).into(),
                            )?;
                            if tx_pointer != &coin.tx_pointer {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx.id(),
                                })
                            }
                        }
                        Input::Contract(Contract {
                            utxo_id,
                            balance_root,
                            state_root,
                            contract_id,
                            tx_pointer,
                            ..
                        }) => {
                            let mut contract = ContractRef::new(&mut *db, *contract_id);
                            let provided_info = ContractUtxoInfo {
                                utxo_id: *utxo_id,
                                tx_pointer: *tx_pointer,
                            };
                            if provided_info
                                != contract.validated_utxo(self.config.utxo_validation)?
                            {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx.id(),
                                })
                            }
                            if balance_root != &contract.balance_root()? {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx.id(),
                                })
                            }
                            if state_root != &contract.state_root()? {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx.id(),
                                })
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    // TODO: Maybe we need move it to `fuel-vm`? O_o Because other `Outputs` are processed there
    /// Computes all zeroed or variable outputs.
    /// In production mode, updates the outputs with computed values.
    /// In validation mode, compares the outputs with computed inputs.
    fn compute_not_utxo_outputs<Tx>(
        &self,
        tx: ExecutionTypes<&mut Tx, &Tx>,
        db: &mut Database,
    ) -> ExecutorResult<()>
    where
        Tx: ExecutableTransaction,
    {
        match tx {
            ExecutionTypes::Production(tx) => {
                // TODO: Inputs, in most cases, are heavier than outputs, so cloning them, but we
                //  to avoid it in the future.
                let mut outputs = tx.outputs().clone();
                for output in outputs.iter_mut() {
                    if let Output::Contract {
                        ref mut balance_root,
                        ref mut state_root,
                        ref input_index,
                    } = output
                    {
                        let contract_id = if let Some(Input::Contract(Contract {
                            contract_id,
                            ..
                        })) = tx.inputs().get(*input_index as usize)
                        {
                            contract_id
                        } else {
                            return Err(ExecutorError::InvalidTransactionOutcome {
                                transaction_id: tx.id(),
                            })
                        };

                        let mut contract = ContractRef::new(&mut *db, *contract_id);
                        *balance_root = contract.balance_root()?;
                        *state_root = contract.state_root()?;
                    }
                }
                *tx.outputs_mut() = outputs;
            }
            ExecutionTypes::Validation(tx) => {
                for output in tx.outputs() {
                    if let Output::Contract {
                        balance_root,
                        state_root,
                        input_index,
                    } = output
                    {
                        let contract_id = if let Some(Input::Contract(Contract {
                            contract_id,
                            ..
                        })) = tx.inputs().get(*input_index as usize)
                        {
                            contract_id
                        } else {
                            return Err(ExecutorError::InvalidTransactionOutcome {
                                transaction_id: tx.id(),
                            })
                        };

                        let mut contract = ContractRef::new(&mut *db, *contract_id);
                        if balance_root != &contract.balance_root()? {
                            return Err(ExecutorError::InvalidTransactionOutcome {
                                transaction_id: tx.id(),
                            })
                        }
                        if state_root != &contract.state_root()? {
                            return Err(ExecutorError::InvalidTransactionOutcome {
                                transaction_id: tx.id(),
                            })
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_coin_or_default(
        &self,
        db: &mut Database,
        utxo_id: UtxoId,
        owner: Address,
        amount: u64,
        asset_id: AssetId,
        maturity: BlockHeight,
    ) -> ExecutorResult<CompressedCoin> {
        if self.config.utxo_validation {
            db.storage::<Coins>()
                .get(&utxo_id)?
                .ok_or(ExecutorError::TransactionValidity(
                    TransactionValidityError::CoinDoesNotExist(utxo_id),
                ))
                .map(Cow::into_owned)
        } else {
            // if utxo validation is disabled, just assign this new input to the original block
            Ok(CompressedCoin {
                owner,
                amount,
                asset_id,
                maturity,
                tx_pointer: Default::default(),
            })
        }
    }

    /// Log a VM backtrace if configured to do so
    fn log_backtrace<Tx>(&self, vm: &Interpreter<VmDatabase, Tx>, receipts: &[Receipt]) {
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
                    hex::encode(&backtrace.memory()[..backtrace.registers()[RegId::SP] as usize]), // print stack
                );
            }
        }
    }

    fn persist_output_utxos(
        &self,
        block_height: BlockHeight,
        tx_idx: u16,
        tx_id: &Bytes32,
        db: &mut Database,
        inputs: &[Input],
        outputs: &[Output],
    ) -> ExecutorResult<()> {
        for (output_index, output) in outputs.iter().enumerate() {
            let utxo_id = UtxoId::new(*tx_id, output_index as u8);
            match output {
                Output::Coin {
                    amount,
                    asset_id,
                    to,
                } => Self::insert_coin(
                    block_height,
                    tx_idx,
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
                    if let Some(Input::Contract(Contract { contract_id, .. })) =
                        inputs.get(*input_idx as usize)
                    {
                        db.storage::<ContractsLatestUtxo>().insert(
                            contract_id,
                            &ContractUtxoInfo {
                                utxo_id,
                                tx_pointer: TxPointer::new(block_height.into(), tx_idx),
                            },
                        )?;
                    } else {
                        return Err(ExecutorError::TransactionValidity(
                            TransactionValidityError::InvalidContractInputIndex(utxo_id),
                        ))
                    }
                }
                Output::Change {
                    to,
                    asset_id,
                    amount,
                } => Self::insert_coin(
                    block_height,
                    tx_idx,
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
                } => Self::insert_coin(
                    block_height,
                    tx_idx,
                    utxo_id,
                    amount,
                    asset_id,
                    to,
                    db,
                )?,
                Output::ContractCreated { contract_id, .. } => {
                    db.storage::<ContractsLatestUtxo>().insert(
                        contract_id,
                        &ContractUtxoInfo {
                            utxo_id,
                            tx_pointer: TxPointer::new(block_height.into(), tx_idx),
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    fn insert_coin(
        block_height: BlockHeight,
        tx_idx: u16,
        utxo_id: UtxoId,
        amount: &Word,
        asset_id: &AssetId,
        to: &Address,
        db: &mut Database,
    ) -> ExecutorResult<()> {
        // Only insert a coin output if it has some amount.
        // This is because variable or transfer outputs won't have any value
        // if there's a revert or panic and shouldn't be added to the utxo set.
        if *amount > Word::MIN {
            let coin = CompressedCoin {
                owner: *to,
                amount: *amount,
                asset_id: *asset_id,
                maturity: 0u32.into(),
                tx_pointer: TxPointer::new(block_height.into(), tx_idx),
            };

            if db.storage::<Coins>().insert(&utxo_id, &coin)?.is_some() {
                return Err(ExecutorError::OutputAlreadyExists)
            }
        }

        Ok(())
    }

    fn persist_receipts(
        &self,
        tx_id: &Bytes32,
        receipts: &[Receipt],
        db: &mut Database,
    ) -> ExecutorResult<()> {
        if db.storage::<Receipts>().insert(tx_id, receipts)?.is_some() {
            return Err(ExecutorError::OutputAlreadyExists)
        }
        Ok(())
    }

    /// Associate all transactions within a block to their respective UTXO owners
    fn index_tx_owners_for_block(
        &self,
        block: &Block,
        block_db_transaction: &mut DatabaseTransaction,
    ) -> ExecutorResult<()> {
        for (tx_idx, tx) in block.transactions().iter().enumerate() {
            let block_height = *block.header().height();
            let mut inputs = &[][..];
            let outputs;
            let tx_id = tx.id();
            match tx {
                Transaction::Script(tx) => {
                    inputs = tx.inputs().as_slice();
                    outputs = tx.outputs().as_slice();
                }
                Transaction::Create(tx) => {
                    inputs = tx.inputs().as_slice();
                    outputs = tx.outputs().as_slice();
                }
                Transaction::Mint(tx) => {
                    outputs = tx.outputs().as_slice();
                }
            }
            self.persist_owners_index(
                block_height,
                inputs,
                outputs,
                &tx_id,
                tx_idx as u16,
                block_db_transaction.deref_mut(),
            )?;
        }
        Ok(())
    }

    /// Index the tx id by owner for all of the inputs and outputs
    fn persist_owners_index(
        &self,
        block_height: BlockHeight,
        inputs: &[Input],
        outputs: &[Output],
        tx_id: &Bytes32,
        tx_idx: u16,
        db: &mut Database,
    ) -> ExecutorResult<()> {
        let mut owners = vec![];
        for input in inputs {
            if let Input::CoinSigned(CoinSigned { owner, .. })
            | Input::CoinPredicate(CoinPredicate { owner, .. }) = input
            {
                owners.push(owner);
            }
        }

        for output in outputs {
            match output {
                Output::Coin { to, .. }
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
        result: &ExecutionResult,
        db: &Database,
    ) -> ExecutorResult<()> {
        let time = result.block.header().time();
        let block_id = result.block.id();
        for TransactionExecutionStatus { id, result } in result.tx_status.iter() {
            match result {
                TransactionExecutionResult::Success { result } => {
                    db.update_tx_status(
                        id,
                        TransactionStatus::Success {
                            block_id,
                            time,
                            result: *result,
                        },
                    )?;
                }
                TransactionExecutionResult::Failed { result, reason } => {
                    db.update_tx_status(
                        id,
                        TransactionStatus::Failed {
                            block_id,
                            time,
                            result: *result,
                            reason: reason.clone(),
                        },
                    )?;
                }
            }
        }
        Ok(())
    }
}

trait Fee {
    fn max_fee(&self) -> Word;

    fn min_fee(&self) -> Word;
}

impl Fee for ScriptCheckedMetadata {
    fn max_fee(&self) -> Word {
        self.fee.total()
    }

    fn min_fee(&self) -> Word {
        self.fee.bytes()
    }
}

impl Fee for CreateCheckedMetadata {
    fn max_fee(&self) -> Word {
        self.fee.total()
    }

    fn min_fee(&self) -> Word {
        self.fee.bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::tables::Messages;
    use fuel_core_types::{
        blockchain::header::ConsensusHeader,
        entities::message::Message,
        fuel_asm::op,
        fuel_crypto::SecretKey,
        fuel_merkle::sparse,
        fuel_tx,
        fuel_tx::{
            field::{
                Inputs,
                Outputs,
            },
            Buildable,
            Chargeable,
            CheckError,
            ConsensusParameters,
            Create,
            Finalizable,
            Script,
            Transaction,
            TransactionBuilder,
        },
        fuel_types::{
            bytes::SerializableVec,
            ContractId,
            Salt,
        },
        fuel_vm::{
            script_with_data_offset,
            util::test_helpers::TestBuilder as TxBuilder,
            Call,
            CallFrame,
        },
        tai64::Tai64,
    };
    use itertools::Itertools;
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };

    pub(crate) fn setup_executable_script() -> (Create, Script) {
        let mut rng = StdRng::seed_from_u64(2322);
        let asset_id: AssetId = rng.gen();
        let owner: Address = rng.gen();
        let input_amount = 1000;
        let variable_transfer_amount = 100;
        let coin_output_amount = 150;

        let (create, contract_id) = create_contract(
            vec![
                // load amount of coins to 0x10
                op::addi(0x10, RegId::FP, CallFrame::a_offset().try_into().unwrap()),
                op::lw(0x10, 0x10, 0),
                // load asset id to 0x11
                op::addi(0x11, RegId::FP, CallFrame::b_offset().try_into().unwrap()),
                op::lw(0x11, 0x11, 0),
                // load address to 0x12
                op::addi(0x12, 0x11, 32),
                // load output index (0) to 0x13
                op::addi(0x13, RegId::ZERO, 0),
                op::tro(0x12, 0x13, 0x10, 0x11),
                op::ret(RegId::ONE),
            ]
            .into_iter()
            .collect::<Vec<u8>>(),
            &mut rng,
        );
        let (script, data_offset) = script_with_data_offset!(
            data_offset,
            vec![
                // set reg 0x10 to call data
                op::movi(0x10, data_offset + 64),
                // set reg 0x11 to asset id
                op::movi(0x11, data_offset),
                // set reg 0x12 to call amount
                op::movi(0x12, variable_transfer_amount),
                // call contract without any tokens to transfer in (3rd arg arbitrary when 2nd is zero)
                op::call(0x10, 0x12, 0x11, RegId::CGAS),
                op::ret(RegId::ONE),
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

        let script = TxBuilder::new(2322)
            .gas_limit(ConsensusParameters::DEFAULT.max_gas_per_tx)
            .start_script(script, script_data)
            .contract_input(contract_id)
            .coin_input(asset_id, input_amount)
            .variable_output(Default::default())
            .coin_output(asset_id, coin_output_amount)
            .change_output(asset_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone();

        (create, script)
    }

    pub(crate) fn test_block(num_txs: usize) -> Block {
        let transactions = (1..num_txs + 1)
            .map(|i| {
                TxBuilder::new(2322u64)
                    .gas_limit(10)
                    .coin_input(AssetId::default(), (i as Word) * 100)
                    .coin_output(AssetId::default(), (i as Word) * 50)
                    .change_output(AssetId::default())
                    .build()
                    .transaction()
                    .clone()
                    .into()
            })
            .collect_vec();

        let mut block = Block::default();
        *block.transactions_mut() = transactions;
        block
    }

    pub(crate) fn create_contract<R: Rng>(
        contract_code: Vec<u8>,
        rng: &mut R,
    ) -> (Create, ContractId) {
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
    #[test]
    fn executor_validates_correctly_produced_block() {
        let producer = Executor::test(Default::default(), Config::local_node());
        let verifier = Executor::test(Default::default(), Config::local_node());
        let block = test_block(10);

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer
            .execute_and_commit(ExecutionTypes::Production(block.into()))
            .unwrap();

        let validation_result =
            verifier.execute_and_commit(ExecutionTypes::Validation(block));
        assert!(validation_result.is_ok());
        assert!(skipped_transactions.is_empty());
    }

    // Ensure transaction commitment != default after execution
    #[test]
    fn executor_commits_transactions_to_block() {
        let producer = Executor::test(Default::default(), Config::local_node());
        let block = test_block(10);
        let start_block = block.clone();

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = producer
            .execute_and_commit(ExecutionBlock::Production(block.into()))
            .unwrap();

        assert!(skipped_transactions.is_empty());
        assert_ne!(
            start_block.header().transactions_root,
            block.header().transactions_root
        );
        assert_eq!(block.transactions().len(), 11);
        assert!(block.transactions()[0].as_mint().is_some());
        assert_eq!(
            block.transactions()[0].as_mint().unwrap().outputs().len(),
            1
        );
        if let Some(Output::Coin {
            asset_id,
            amount,
            to,
        }) = block.transactions()[0].as_mint().unwrap().outputs().first()
        {
            assert_eq!(asset_id, &AssetId::BASE);
            // Expected fee is zero, because price is zero.
            assert_eq!(*amount, 0);
            assert_eq!(to, &Address::zeroed());
        } else {
            panic!("Invalid outputs of coinbase");
        }
    }

    mod coinbase {
        use super::*;
        use fuel_core_types::fuel_asm::GTFArgs;

        #[test]
        fn executor_commits_transactions_with_non_zero_coinbase_generation() {
            let price = 1;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(2322u64)
                .gas_limit(limit)
                // Set a price for the test
                .gas_price(price)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let mut producer = Executor::test(Default::default(), Config::local_node());
            let recipient = [1u8; 32].into();
            producer.config.block_producer.coinbase_recipient = recipient;
            producer
                .config
                .chain_conf
                .transaction_parameters
                .gas_price_factor = gas_price_factor;

            let expected_fee_amount = TransactionFee::checked_from_values(
                &producer.config.chain_conf.transaction_parameters,
                script.metered_bytes_size() as Word,
                limit,
                price,
            )
            .unwrap()
            .total();
            let invalid_duplicate_tx = script.clone().into();

            let mut block = Block::default();
            *block.transactions_mut() = vec![script.into(), invalid_duplicate_tx];

            let ExecutionResult {
                block,
                skipped_transactions,
                ..
            } = producer
                .execute_and_commit(ExecutionBlock::Production(block.into()))
                .unwrap();

            assert_eq!(skipped_transactions.len(), 1);
            assert_eq!(block.transactions().len(), 2);
            assert!(block.transactions()[0].as_mint().is_some());
            assert_eq!(
                block.transactions()[0].as_mint().unwrap().outputs().len(),
                1
            );
            if let Some(Output::Coin {
                asset_id,
                amount,
                to,
            }) = block.transactions()[0].as_mint().unwrap().outputs().first()
            {
                assert_eq!(asset_id, &AssetId::BASE);
                assert!(expected_fee_amount > 0);
                assert_eq!(*amount, expected_fee_amount);
                assert_eq!(to, &recipient);
            } else {
                panic!("Invalid outputs of coinbase");
            }
        }

        #[test]
        fn executor_commits_transactions_with_non_zero_coinbase_validation() {
            let price = 1;
            let limit = 0;
            let gas_price_factor = 1;
            let script = TxBuilder::new(2322u64)
                .gas_limit(limit)
                // Set a price for the test
                .gas_price(price)
                .coin_input(AssetId::BASE, 10000)
                .change_output(AssetId::BASE)
                .build()
                .transaction()
                .clone();

            let mut producer = Executor::test(Default::default(), Config::local_node());
            let recipient = [1u8; 32].into();
            producer.config.block_producer.coinbase_recipient = recipient;
            producer
                .config
                .chain_conf
                .transaction_parameters
                .gas_price_factor = gas_price_factor;

            let mut block = Block::default();
            *block.transactions_mut() = vec![script.into()];

            let ExecutionResult {
                block: produced_block,
                skipped_transactions,
                ..
            } = producer
                .execute_and_commit(ExecutionBlock::Production(block.into()))
                .unwrap();
            assert!(skipped_transactions.is_empty());
            let produced_txs = produced_block.transactions().to_vec();

            let validator = Executor::test(
                Default::default(),
                // Use the same config as block producer
                producer.config,
            );
            let ExecutionResult {
                block: validated_block,
                ..
            } = validator
                .execute_and_commit(ExecutionBlock::Validation(produced_block))
                .unwrap();
            assert_eq!(validated_block.transactions(), produced_txs);
            let (_, owned_transactions_td_id) = validator
                .database
                .owned_transactions(recipient, None, None)
                .next()
                .unwrap()
                .unwrap();
            // Should own `Mint` transaction
            assert_eq!(owned_transactions_td_id, produced_txs[0].id());
        }

        #[test]
        fn execute_cb_command() {
            fn compare_coinbase_addresses(
                config_coinbase: Address,
                expected_in_tx_coinbase: Address,
            ) -> bool {
                let script = TxBuilder::new(2322u64)
                    .gas_limit(100000)
                    // Set a price for the test
                    .gas_price(0)
                    .start_script(vec![
                        // Store the size of the `Address`(32 bytes) into register `0x11`.
                        op::movi(0x11, Address::LEN.try_into().unwrap()),
                        // Allocate 32 bytes on the heap.
                        op::aloc(0x11),
                        // Store the pointer to the beginning of the free memory into 
                        // register `0x10`. It requires shifting of `RegId::HP` by 1 to point
                        // on the free memory.
                        op::addi(0x10, RegId::HP, 0),
                        // Store `config_coinbase` `Address` into MEM[$0x10; 32].
                        op::cb(0x10),
                        // Store the pointer on the beginning of script data into register `0x12`.
                        // Script data contains `expected_in_tx_coinbase` - 32 bytes of data.
                        op::gtf_args(0x12, 0x00, GTFArgs::ScriptData),
                        // Compare retrieved `config_coinbase`(register `0x10`) with 
                        // passed `expected_in_tx_coinbase`(register `0x12`) where the length 
                        // of memory comparison is 32 bytes(register `0x11`) and store result into
                        // register `0x13`(1 - true, 0 - false). 
                        op::meq(0x13, 0x10, 0x12, 0x11),
                        // Return the result of the comparison as a receipt.
                        op::ret(0x13)
                    ], expected_in_tx_coinbase.to_vec() /* pass expected address as script data */)
                    .coin_input(AssetId::BASE, 1000)
                    .variable_output(Default::default())
                    .coin_output(AssetId::BASE, 1000)
                    .change_output(AssetId::BASE)
                    .build()
                    .transaction()
                    .clone();

                let mut producer =
                    Executor::test(Default::default(), Config::local_node());
                producer.config.block_producer.coinbase_recipient = config_coinbase;

                let mut block = Block::default();
                *block.transactions_mut() = vec![script.clone().into()];

                assert!(producer
                    .execute_and_commit(ExecutionBlock::Production(block.into()))
                    .is_ok());
                let receipts = producer
                    .database
                    .storage::<Receipts>()
                    .get(&script.id())
                    .unwrap()
                    .unwrap();

                if let Some(Receipt::Return { val, .. }) = receipts.get(0) {
                    *val == 1
                } else {
                    panic!("Execution of the `CB` script failed failed")
                }
            }

            assert!(compare_coinbase_addresses(
                Address::from([1u8; 32]),
                Address::from([1u8; 32])
            ));
            assert!(!compare_coinbase_addresses(
                Address::from([9u8; 32]),
                Address::from([1u8; 32])
            ));
            assert!(!compare_coinbase_addresses(
                Address::from([1u8; 32]),
                Address::from([9u8; 32])
            ));
            assert!(compare_coinbase_addresses(
                Address::from([9u8; 32]),
                Address::from([9u8; 32])
            ));
        }

        #[test]
        fn invalidate_is_not_first() {
            let mint = Transaction::mint(TxPointer::new(0, 1), vec![]);

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::CoinbaseIsNotFirstTransaction
            ));
        }

        #[test]
        fn invalidate_block_height() {
            let mint = Transaction::mint(TxPointer::new(1, 0), vec![]);

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::InvalidTransaction(
                    CheckError::TransactionMintIncorrectBlockHeight
                )
            ));
        }

        #[test]
        fn invalidate_zero_outputs() {
            let mint = Transaction::mint(TxPointer::new(0, 0), vec![]);

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::CoinbaseOutputIsInvalid
            ));
        }

        #[test]
        fn invalidate_more_than_one_outputs() {
            let mint = Transaction::mint(
                TxPointer::new(0, 0),
                vec![
                    Output::coin(Address::from([1u8; 32]), 0, AssetId::from([3u8; 32])),
                    Output::coin(Address::from([2u8; 32]), 0, AssetId::from([4u8; 32])),
                ],
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::CoinbaseSeveralOutputs
            ));
        }

        #[test]
        fn invalidate_not_base_asset() {
            let mint = Transaction::mint(
                TxPointer::new(0, 0),
                vec![Output::coin(
                    Address::from([1u8; 32]),
                    0,
                    AssetId::from([3u8; 32]),
                )],
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::CoinbaseOutputIsInvalid
            ));
        }

        #[test]
        fn invalidate_mismatch_amount() {
            let mint = Transaction::mint(
                TxPointer::new(0, 0),
                vec![Output::coin(Address::from([1u8; 32]), 123, AssetId::BASE)],
            );

            let mut block = Block::default();
            *block.transactions_mut() = vec![mint.into()];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::CoinbaseAmountMismatch
            ));
        }

        #[test]
        fn invalidate_more_than_one_mint_is_not_allowed() {
            let mut block = Block::default();
            *block.transactions_mut() = vec![
                Transaction::mint(
                    TxPointer::new(0, 0),
                    vec![Output::coin(Address::from([1u8; 32]), 0, AssetId::BASE)],
                )
                .into(),
                Transaction::mint(
                    TxPointer::new(0, 0),
                    vec![Output::coin(Address::from([2u8; 32]), 0, AssetId::BASE)],
                )
                .into(),
            ];
            block.header_mut().recalculate_metadata();

            let validator = Executor::test(Default::default(), Config::local_node());
            let validation_err = validator
                .execute_and_commit(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                ExecutorError::NotSupportedTransaction(_)
            ));
        }
    }

    // Ensure tx has at least one input to cover gas
    #[test]
    fn executor_invalidates_missing_gas_input() {
        let producer = Executor::test(Default::default(), Config::local_node());
        let factor = producer
            .config
            .chain_conf
            .transaction_parameters
            .gas_price_factor as f64;

        let verifier = Executor::test(Default::default(), Config::local_node());

        let gas_limit = 100;
        let gas_price = 1;
        let mut tx = Script::default();
        tx.set_gas_limit(gas_limit);
        tx.set_gas_price(gas_price);
        let tx: Transaction = tx.into();

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let mut block_db_transaction = producer.database.transaction();
        let ExecutionData {
            skipped_transactions,
            ..
        } = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .unwrap();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::InvalidTransaction(CheckError::InsufficientFeeAmount { expected, .. }) if expected == (gas_limit as f64 / factor).ceil() as u64
        ));

        // Produced block is valid
        let mut block_db_transaction = verifier.database.transaction();
        verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .unwrap();

        // Invalidate the block with Insufficient tx
        block.transactions.push(tx);
        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier.execute_transactions(
            &mut block_db_transaction,
            ExecutionType::Validation(&mut block),
        );
        assert!(matches!(
            verify_result,
            Err(ExecutorError::InvalidTransaction(CheckError::InsufficientFeeAmount { expected, ..})) if expected == (gas_limit as f64 / factor).ceil() as u64
        ))
    }

    #[test]
    fn executor_invalidates_duplicate_tx_id() {
        let producer = Executor::test(Default::default(), Config::local_node());

        let verifier = Executor::test(Default::default(), Config::local_node());

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![Transaction::default(), Transaction::default()],
        };

        let mut block_db_transaction = producer.database.transaction();
        let ExecutionData {
            skipped_transactions,
            ..
        } = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .unwrap();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::TransactionIdCollision(_)
        ));

        // Produced block is valid
        let mut block_db_transaction = verifier.database.transaction();
        verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .unwrap();

        // Make the block invalid by adding of the duplicating transaction
        block.transactions.push(Transaction::default());
        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier.execute_transactions(
            &mut block_db_transaction,
            ExecutionType::Validation(&mut block.clone()),
        );
        assert!(matches!(
            verify_result,
            Err(ExecutorError::TransactionIdCollision(_))
        ));
    }

    // invalidate a block if a tx input doesn't exist
    #[test]
    fn executor_invalidates_missing_inputs() {
        // create an input which doesn't exist in the utxo set
        let mut rng = StdRng::seed_from_u64(2322u64);

        let tx = TransactionBuilder::script(
            vec![op::ret(RegId::ONE)].into_iter().collect(),
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
        .finalize_as_transaction();

        // setup executors with utxo-validation enabled
        let config = Config {
            utxo_validation: true,
            ..Config::local_node()
        };
        let producer = Executor::test(Database::default(), config.clone());

        let verifier = Executor::test(Default::default(), config);

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.clone()],
        };

        let mut block_db_transaction = producer.database.transaction();
        let ExecutionData {
            skipped_transactions,
            ..
        } = producer
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .unwrap();
        let produce_result = &skipped_transactions[0].1;
        assert!(matches!(
            produce_result,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            )
        ));

        // Produced block is valid
        let mut block_db_transaction = verifier.database.transaction();
        verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .unwrap();

        // Invalidate block by adding transaction with not existing coin
        block.transactions.push(tx);
        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier.execute_transactions(
            &mut block_db_transaction,
            ExecutionType::Validation(&mut block),
        );
        assert!(matches!(
            verify_result,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::CoinDoesNotExist(_)
            ))
        ));
    }

    // corrupt a produced block by randomizing change amount
    // and verify that the executor invalidates the tx
    #[test]
    fn executor_invalidates_blocks_with_diverging_tx_outputs() {
        let input_amount = 10;
        let fake_output_amount = 100;

        let tx: Transaction = TxBuilder::new(2322u64)
            .gas_limit(1)
            .coin_input(Default::default(), input_amount)
            .change_output(Default::default())
            .build()
            .transaction()
            .clone()
            .into();

        let tx_id = tx.id();

        let producer = Executor::test(Default::default(), Config::local_node());

        let verifier = Executor::test(Default::default(), Config::local_node());

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx];

        let ExecutionResult { mut block, .. } = producer
            .execute_and_commit(ExecutionBlock::Production(block.into()))
            .unwrap();

        // modify change amount
        if let Transaction::Script(script) = &mut block.transactions_mut()[1] {
            if let Output::Change { amount, .. } = &mut script.outputs_mut()[0] {
                *amount = fake_output_amount
            }
        }

        let verify_result =
            verifier.execute_and_commit(ExecutionBlock::Validation(block));
        assert!(matches!(
            verify_result,
            Err(ExecutorError::InvalidTransactionOutcome { transaction_id }) if transaction_id == tx_id
        ));
    }

    // corrupt the merkle sum tree commitment from a produced block and verify that the
    // validation logic will reject the block
    #[test]
    fn executor_invalidates_blocks_with_diverging_tx_commitment() {
        let mut rng = StdRng::seed_from_u64(2322u64);
        let tx: Transaction = TxBuilder::new(2322u64)
            .gas_limit(1)
            .coin_input(Default::default(), 10)
            .change_output(Default::default())
            .build()
            .transaction()
            .clone()
            .into();

        let producer = Executor::test(Default::default(), Config::local_node());

        let verifier = Executor::test(Default::default(), Config::local_node());

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx];

        let ExecutionResult { mut block, .. } = producer
            .execute_and_commit(ExecutionBlock::Production(block.into()))
            .unwrap();

        // randomize transaction commitment
        block.header_mut().application.generated.transactions_root = rng.gen();
        block.header_mut().recalculate_metadata();

        let verify_result =
            verifier.execute_and_commit(ExecutionBlock::Validation(block));

        assert!(matches!(verify_result, Err(ExecutorError::InvalidBlockId)))
    }

    // invalidate a block if a tx is missing at least one coin input
    #[test]
    fn executor_invalidates_missing_coin_input() {
        let tx: Transaction =
            TxBuilder::new(2322u64).build().transaction().clone().into();
        let tx_id = tx.id();

        let executor = Executor::test(
            Database::default(),
            Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult {
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        let err = &skipped_transactions[0].1;
        // assert block failed to validate when transaction didn't contain any coin inputs
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(TransactionValidityError::NoCoinOrMessageInput(id)) if id == tx_id
        ));
    }

    #[test]
    fn skipped_tx_not_changed_spent_status() {
        // `tx2` has two inputs: one used by `tx1` and on random. So after the execution of `tx1`,
        // the `tx2` become invalid and should be skipped by the block producers. Skipped
        // transactions should not affect the state so the second input should be `Unspent`.
        // # Dev-note: `TxBuilder::new(2322u64)` is used to create transactions, it produces
        // the same first input.
        let tx1 = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone();

        let tx2 = TxBuilder::new(2322u64)
            // The same input as `tx1`
            .coin_input(AssetId::default(), 100)
            // Additional unique for `tx2` input
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone();

        let first_input = tx2.inputs()[0].clone();
        let second_input = tx2.inputs()[1].clone();
        let db = &mut Database::default();
        // Insert both inputs
        db.storage::<Coins>()
            .insert(
                &first_input.utxo_id().unwrap().clone(),
                &CompressedCoin {
                    owner: *first_input.input_owner().unwrap(),
                    amount: 100,
                    asset_id: AssetId::default(),
                    maturity: Default::default(),
                    tx_pointer: Default::default(),
                },
            )
            .unwrap();
        db.storage::<Coins>()
            .insert(
                &second_input.utxo_id().unwrap().clone(),
                &CompressedCoin {
                    owner: *second_input.input_owner().unwrap(),
                    amount: 100,
                    asset_id: AssetId::default(),
                    maturity: Default::default(),
                    tx_pointer: Default::default(),
                },
            )
            .unwrap();
        let executor = Executor::test(
            db.clone(),
            Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1.into(), tx2.clone().into()],
        };

        // The first input should be `Unspent` before execution.
        db.storage::<Coins>()
            .get(first_input.utxo_id().unwrap())
            .unwrap()
            .expect("coin should be unspent");
        // The second input should be `Unspent` before execution.
        db.storage::<Coins>()
            .get(second_input.utxo_id().unwrap())
            .unwrap()
            .expect("coin should be unspent");

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();
        // `tx2` should be skipped.
        assert_eq!(block.transactions().len(), 2 /* coinbase and `tx1` */);
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(
            skipped_transactions[0].0.as_script().unwrap().id(),
            tx2.id()
        );

        // The first input should be spent by `tx1` after execution.
        let coin = db
            .storage::<Coins>()
            .get(first_input.utxo_id().unwrap())
            .unwrap();
        // verify coin is pruned from utxo set
        assert!(coin.is_none());
        // The second input should be `Unspent` after execution.
        db.storage::<Coins>()
            .get(second_input.utxo_id().unwrap())
            .unwrap()
            .expect("coin should be unspent");
    }

    #[test]
    fn skipped_txs_not_affect_order() {
        // `tx1` is invalid because it doesn't have inputs for gas.
        // `tx2` is a `Create` transaction with some code inside.
        // `tx3` is a `Script` transaction that depends on `tx2`. It will be skipped
        // if `tx2` is not executed before `tx3`.
        //
        // The test checks that execution for the block with transactions [tx1, tx2, tx3] skips
        // transaction `tx1` and produce a block [tx2, tx3] with the expected order.
        let mut tx1 = Script::default();
        tx1.set_gas_limit(1000000);
        tx1.set_gas_price(1000000);
        let (tx2, tx3) = setup_executable_script();

        let executor = Executor::test(Default::default(), Config::local_node());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![
                tx1.clone().into(),
                tx2.clone().into(),
                tx3.clone().into(),
            ],
        };

        let ExecutionResult {
            block,
            skipped_transactions,
            ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();
        assert_eq!(
            block.transactions().len(),
            3 // coinbase, `tx2` and `tx3`
        );
        assert_eq!(block.transactions()[1].id(), tx2.id());
        assert_eq!(block.transactions()[2].id(), tx3.id());
        // `tx1` should be skipped.
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(skipped_transactions[0].0.as_script(), Some(&tx1));
        let tx2_index_in_the_block =
            block.transactions()[2].as_script().unwrap().inputs()[0]
                .tx_pointer()
                .unwrap()
                .tx_index();
        assert_eq!(tx2_index_in_the_block, 1);
    }

    #[test]
    fn input_coins_are_marked_as_spent() {
        // ensure coins are marked as spent after tx is processed
        let tx: Transaction = TxBuilder::new(2322u64)
            .coin_input(AssetId::default(), 100)
            .change_output(AssetId::default())
            .build()
            .transaction()
            .clone()
            .into();

        let db = &Database::default();
        let executor = Executor::test(db.clone(), Config::local_node());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult { block, .. } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(
                block.transactions()[1].as_script().unwrap().inputs()[0]
                    .utxo_id()
                    .unwrap(),
            )
            .unwrap();
        // spent coins should be removed
        assert!(coin.is_none());
    }

    #[test]
    fn contracts_balance_and_state_roots_no_modifications_updated() {
        // Values in inputs and outputs are random. If the execution of the transaction successful,
        // it should actualize them to use a valid the balance and state roots. Because it is not
        // changes, the balance the root should be default - `[0; 32]`.
        let mut rng = StdRng::seed_from_u64(2322u64);

        let (create, contract_id) = create_contract(vec![], &mut rng);
        let non_modify_state_tx: Transaction = TxBuilder::new(2322)
            .gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(vec![op::ret(1)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();
        let db = &mut Database::default();

        let executor = Executor::test(
            db.clone(),
            Config {
                utxo_validation: false,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), non_modify_state_tx],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        // Assert the balance and state roots should be the same before and after execution.
        let empty_state = (*sparse::empty_sum()).into();
        let executed_tx = block.transactions()[2].as_script().unwrap();
        assert!(matches!(
            tx_status[2].result,
            TransactionExecutionResult::Success { .. }
        ));
        assert_eq!(executed_tx.inputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.inputs()[0].balance_root(), Some(&empty_state));
        assert_eq!(executed_tx.outputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.outputs()[0].balance_root(), Some(&empty_state));

        let expected_tx = block.transactions()[2].clone();
        let storage_tx = executor
            .database
            .storage::<Transactions>()
            .get(&executed_tx.id())
            .unwrap()
            .unwrap()
            .into_owned();
        assert_eq!(storage_tx, expected_tx);
    }

    #[test]
    fn contracts_balance_and_state_roots_updated_no_modifications_on_fail() {
        // Values in inputs and outputs are random. If the execution of the transaction fails,
        // it still should actualize them to use the balance and state roots before the execution.
        let mut rng = StdRng::seed_from_u64(2322u64);

        let (create, contract_id) = create_contract(vec![], &mut rng);
        // The transaction with invalid script.
        let non_modify_state_tx: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::add(RegId::PC, RegId::PC, RegId::PC)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();
        let db = &mut Database::default();

        let executor = Executor::test(
            db.clone(),
            Config {
                utxo_validation: false,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), non_modify_state_tx],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        // Assert the balance and state roots should be the same before and after execution.
        let empty_state = (*sparse::empty_sum()).into();
        let executed_tx = block.transactions()[2].as_script().unwrap();
        assert!(matches!(
            tx_status[2].result,
            TransactionExecutionResult::Failed { .. }
        ));
        assert_eq!(
            executed_tx.inputs()[0].state_root(),
            executed_tx.outputs()[0].state_root()
        );
        assert_eq!(
            executed_tx.inputs()[0].balance_root(),
            executed_tx.outputs()[0].balance_root()
        );
        assert_eq!(executed_tx.inputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.inputs()[0].balance_root(), Some(&empty_state));

        let expected_tx = block.transactions()[2].clone();
        let storage_tx = executor
            .database
            .storage::<Transactions>()
            .get(&expected_tx.id())
            .unwrap()
            .unwrap()
            .into_owned();
        assert_eq!(storage_tx, expected_tx);
    }

    #[test]
    fn contracts_balance_and_state_roots_updated_modifications_updated() {
        // Values in inputs and outputs are random. If the execution of the transaction that
        // modifies the state and the balance is successful, it should update roots.
        let mut rng = StdRng::seed_from_u64(2322u64);

        // Create a contract that modifies the state
        let (create, contract_id) = create_contract(
            vec![
                // Sets the state STATE[0x1; 32] = value of `RegId::PC`;
                op::sww(0x1, 0x29, RegId::PC),
                op::ret(1),
            ]
            .into_iter()
            .collect::<Vec<u8>>(),
            &mut rng,
        );

        let transfer_amount = 100 as Word;
        let asset_id = AssetId::from([2; 32]);
        let (script, data_offset) = script_with_data_offset!(
            data_offset,
            vec![
                // Set register `0x10` to `Call`
                op::movi(0x10, data_offset + AssetId::LEN as u32),
                // Set register `0x11` with offset to data that contains `asset_id`
                op::movi(0x11, data_offset),
                // Set register `0x12` with `transfer_amount`
                op::movi(0x12, transfer_amount as u32),
                op::call(0x10, 0x12, 0x11, RegId::CGAS),
                op::ret(RegId::ONE),
            ],
            ConsensusParameters::DEFAULT.tx_offset()
        );

        let script_data: Vec<u8> = [
            asset_id.as_ref(),
            Call::new(contract_id, transfer_amount, data_offset as Word)
                .to_bytes()
                .as_ref(),
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect();

        let modify_balance_and_state_tx = TxBuilder::new(2322)
            .gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(script, script_data)
            .contract_input(contract_id)
            .coin_input(asset_id, transfer_amount)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone();
        let db = &mut Database::default();

        let executor = Executor::test(
            db.clone(),
            Config {
                utxo_validation: false,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), modify_balance_and_state_tx.into()],
        };

        let ExecutionResult {
            block, tx_status, ..
        } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        let empty_state = (*sparse::empty_sum()).into();
        let executed_tx = block.transactions()[2].as_script().unwrap();
        assert!(matches!(
            tx_status[2].result,
            TransactionExecutionResult::Success { .. }
        ));
        assert_eq!(executed_tx.inputs()[0].state_root(), Some(&empty_state));
        assert_eq!(executed_tx.inputs()[0].balance_root(), Some(&empty_state));
        // Roots should be different
        assert_ne!(
            executed_tx.inputs()[0].state_root(),
            executed_tx.outputs()[0].state_root()
        );
        assert_ne!(
            executed_tx.inputs()[0].balance_root(),
            executed_tx.outputs()[0].balance_root()
        );

        let expected_tx = block.transactions()[2].clone();
        let storage_tx = executor
            .database
            .storage::<Transactions>()
            .get(&expected_tx.id())
            .unwrap()
            .unwrap()
            .into_owned();
        assert_eq!(storage_tx, expected_tx);
    }

    #[test]
    fn foreign_transfer_should_not_affect_balance_root() {
        // The foreign transfer of tokens should not affect the balance root of the transaction.
        let mut rng = StdRng::seed_from_u64(2322u64);

        let (create, contract_id) = create_contract(vec![], &mut rng);

        let transfer_amount = 100 as Word;
        let asset_id = AssetId::from([2; 32]);
        let mut foreign_transfer = TxBuilder::new(2322)
            .gas_limit(10000)
            .coin_input(AssetId::zeroed(), 10000)
            .start_script(vec![op::ret(1)], vec![])
            .coin_input(asset_id, transfer_amount)
            .coin_output(asset_id, transfer_amount)
            .build()
            .transaction()
            .clone();
        if let Some(Output::Coin { to, .. }) = foreign_transfer
            .as_script_mut()
            .unwrap()
            .outputs_mut()
            .last_mut()
        {
            *to = Address::try_from(contract_id.as_ref()).unwrap();
        } else {
            panic!("Last outputs should be a coin for the contract");
        }
        let db = &mut Database::default();

        let executor = Executor::test(
            db.clone(),
            Config {
                utxo_validation: false,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 1u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![create.into(), foreign_transfer.into()],
        };

        let _ = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        // Assert the balance root should not be affected.
        let empty_state = (*sparse::empty_sum()).into();
        assert_eq!(
            ContractRef::new(db, contract_id).balance_root().unwrap(),
            empty_state
        );
    }

    #[test]
    fn input_coins_are_marked_as_spent_with_utxo_validation_enabled() {
        // ensure coins are marked as spent after tx is processed
        let mut rng = StdRng::seed_from_u64(2322u64);
        let starting_block = BlockHeight::from(5u64);
        let starting_block_tx_idx = Default::default();

        let tx = TransactionBuilder::script(
            vec![op::ret(RegId::ONE)].into_iter().collect(),
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
        if let Input::CoinSigned(CoinSigned {
            utxo_id,
            owner,
            amount,
            asset_id,
            ..
        }) = tx.inputs()[0]
        {
            db.storage::<Coins>()
                .insert(
                    &utxo_id,
                    &CompressedCoin {
                        owner,
                        amount,
                        asset_id,
                        maturity: Default::default(),
                        tx_pointer: TxPointer::new(
                            starting_block.into(),
                            starting_block_tx_idx,
                        ),
                    },
                )
                .unwrap();
        }

        let executor = Executor::test(
            db.clone(),
            Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        );

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 6u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.into()],
        };

        let ExecutionResult { block, .. } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(
                block.transactions()[1].as_script().unwrap().inputs()[0]
                    .utxo_id()
                    .unwrap(),
            )
            .unwrap();
        assert!(coin.is_none());
    }

    #[test]
    fn validation_succeeds_when_input_contract_utxo_id_uses_expected_value() {
        let mut rng = StdRng::seed_from_u64(2322);
        // create a contract in block 1
        // verify a block 2 with tx containing contract id from block 1, using the correct contract utxo_id from block 1.
        let (tx, contract_id) = create_contract(vec![], &mut rng);
        let first_block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let tx2: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::ret(1)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();

        let second_block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 2u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx2],
        };

        let db = Database::default();

        let setup = Executor::test(db.clone(), Config::local_node());

        setup
            .execute_and_commit(ExecutionBlock::Production(first_block))
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor::test(producer_view, Config::local_node());
        let ExecutionResult {
            block: second_block,
            ..
        } = producer
            .execute_and_commit(ExecutionBlock::Production(second_block))
            .unwrap();

        let verifier = Executor::test(db, Config::local_node());
        let verify_result =
            verifier.execute_and_commit(ExecutionBlock::Validation(second_block));
        assert!(verify_result.is_ok());
    }

    // verify that a contract input must exist for a transaction
    #[test]
    fn invalidates_if_input_contract_utxo_id_is_divergent() {
        let mut rng = StdRng::seed_from_u64(2322);

        // create a contract in block 1
        // verify a block 2 containing contract id from block 1, with wrong input contract utxo_id
        let (tx, contract_id) = create_contract(vec![], &mut rng);
        let tx2: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::addi(0x10, RegId::ZERO, 0), op::ret(1)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();

        let first_block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into(), tx2],
        };

        let tx3: Transaction = TxBuilder::new(2322)
            .start_script(vec![op::addi(0x10, RegId::ZERO, 1), op::ret(1)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
            .into();
        let tx_id = tx3.id();

        let second_block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: 2u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx3],
        };

        let db = Database::default();

        let setup = Executor::test(db.clone(), Config::local_node());

        setup
            .execute_and_commit(ExecutionBlock::Production(first_block))
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor::test(producer_view, Config::local_node());

        let ExecutionResult {
            block: mut second_block,
            ..
        } = producer
            .execute_and_commit(ExecutionBlock::Production(second_block))
            .unwrap();
        // Corrupt the utxo_id of the contract output
        if let Transaction::Script(script) = &mut second_block.transactions_mut()[1] {
            if let Input::Contract(Contract { utxo_id, .. }) = &mut script.inputs_mut()[0]
            {
                // use a previously valid contract id which isn't the correct one for this block
                *utxo_id = UtxoId::new(tx_id, 0);
            }
        }

        let verifier = Executor::test(db, Config::local_node());
        let verify_result =
            verifier.execute_and_commit(ExecutionBlock::Validation(second_block));

        assert!(matches!(
            verify_result,
            Err(ExecutorError::InvalidTransactionOutcome {
                transaction_id
            }) if transaction_id == tx_id
        ));
    }

    #[test]
    fn outputs_with_amount_are_included_utxo_set() {
        let (deploy, script) = setup_executable_script();
        let script_id = script.id();

        let database = &Database::default();
        let executor = Executor::test(database.clone(), Config::local_node());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![deploy.into(), script.into()],
        };

        let ExecutionResult { block, .. } = executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        // ensure that all utxos with an amount are stored into the utxo set
        for (idx, output) in block.transactions()[2]
            .as_script()
            .unwrap()
            .outputs()
            .iter()
            .enumerate()
        {
            let id = fuel_tx::UtxoId::new(script_id, idx as u8);
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

    #[test]
    fn outputs_with_no_value_are_excluded_from_utxo_set() {
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
            .transaction()
            .clone()
            .into();
        let tx_id = tx.id();

        let database = &Database::default();
        let executor = Executor::test(database.clone(), Config::local_node());

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        for idx in 0..2 {
            let id = UtxoId::new(tx_id, idx);
            let maybe_utxo = database.storage::<Coins>().get(&id).unwrap();
            assert!(maybe_utxo.is_none());
        }
    }

    fn message_from_input(input: &Input, da_height: u64) -> Message {
        Message {
            sender: *input.sender().unwrap(),
            recipient: *input.recipient().unwrap(),
            nonce: *input.nonce().unwrap(),
            amount: input.amount().unwrap(),
            data: input
                .input_data()
                .map(|data| data.to_vec())
                .unwrap_or_default(),
            da_height: DaBlockHeight(da_height),
        }
    }

    /// Helper to build transactions and a message in it for some of the message tests
    fn make_tx_and_message(rng: &mut StdRng, da_height: u64) -> (Transaction, Message) {
        let tx = TransactionBuilder::script(vec![], vec![])
            .add_unsigned_message_input(rng.gen(), rng.gen(), rng.gen(), 1000, vec![])
            .finalize();

        let message = message_from_input(&tx.inputs()[0], da_height);
        (tx.into(), message)
    }

    /// Helper to build database and executor for some of the message tests
    fn make_executor(messages: &[&Message]) -> Executor<Database> {
        let mut database = Database::default();
        let database_ref = &mut database;

        for message in messages {
            database_ref
                .storage::<Messages>()
                .insert(message.id(), message)
                .unwrap();
        }

        Executor::test(
            database,
            Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        )
    }

    #[test]
    fn unspent_message_succeeds_when_msg_da_height_lt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 0);

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult { block, .. } = make_executor(&[&message])
            .execute_and_commit(ExecutionBlock::Production(block))
            .expect("block execution failed unexpectedly");

        make_executor(&[&message])
            .execute_and_commit(ExecutionBlock::Validation(block))
            .expect("block validation failed unexpectedly");
    }

    #[test]
    fn successful_execution_consume_all_messages() {
        let mut rng = StdRng::seed_from_u64(2322);
        let to: Address = rng.gen();
        let amount = 500;

        let tx = TransactionBuilder::script(vec![], vec![])
            // Add `Input::MessageCoin`
            .add_unsigned_message_input(rng.gen(), rng.gen(), rng.gen(), amount, vec![])
            // Add `Input::MessageData`
            .add_unsigned_message_input(rng.gen(), rng.gen(), rng.gen(), amount, vec![0xff; 10])
            .add_output(Output::change(to, amount + amount, AssetId::BASE))
            .finalize();
        let tx_id = tx.id();

        let message_coin = message_from_input(&tx.inputs()[0], 0);
        let message_data = message_from_input(&tx.inputs()[1], 0);
        let messages = vec![&message_coin, &message_data];

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let exec = make_executor(&messages);
        let mut block_db_transaction = exec.database.transaction();
        assert_eq!(block_db_transaction.all_messages(None, None).count(), 2);

        let ExecutionData {
            skipped_transactions,
            ..
        } = exec
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .unwrap();
        assert_eq!(skipped_transactions.len(), 0);

        // Successful execution consumes `message_coin` and `message_data`.
        assert_eq!(block_db_transaction.all_messages(None, None).count(), 0);
        assert!(block_db_transaction
            .is_message_spent(&message_coin.nonce)
            .unwrap());
        assert!(block_db_transaction
            .is_message_spent(&message_data.nonce)
            .unwrap());
        assert_eq!(
            block_db_transaction
                .coin(&UtxoId::new(tx_id, 0))
                .unwrap()
                .amount,
            amount + amount
        );
    }

    #[test]
    fn reverted_execution_consume_only_message_coins() {
        let mut rng = StdRng::seed_from_u64(2322);
        let to: Address = rng.gen();
        let amount = 500;

        // Script that return `1` - failed script -> execution result will be reverted.
        let script = vec![op::ret(1)].into_iter().collect();
        let tx = TransactionBuilder::script(script, vec![])
            // Add `Input::MessageCoin`
            .add_unsigned_message_input(rng.gen(), rng.gen(), rng.gen(), amount, vec![])
            // Add `Input::MessageData`
            .add_unsigned_message_input(rng.gen(), rng.gen(), rng.gen(), amount, vec![0xff; 10])
            .add_output(Output::change(to, amount + amount, AssetId::BASE))
            .finalize();
        let tx_id = tx.id();

        let message_coin = message_from_input(&tx.inputs()[0], 0);
        let message_data = message_from_input(&tx.inputs()[1], 0);
        let messages = vec![&message_coin, &message_data];

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx.into()],
        };

        let exec = make_executor(&messages);
        let mut block_db_transaction = exec.database.transaction();
        assert_eq!(block_db_transaction.all_messages(None, None).count(), 2);

        let ExecutionData {
            skipped_transactions,
            ..
        } = exec
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .unwrap();
        assert_eq!(skipped_transactions.len(), 0);

        // We should spend only `message_coin`. The `message_data` should be unspent.
        assert_eq!(block_db_transaction.all_messages(None, None).count(), 1);
        assert!(block_db_transaction
            .is_message_spent(&message_coin.nonce)
            .unwrap());
        assert!(!block_db_transaction
            .is_message_spent(&message_data.nonce)
            .unwrap());
        assert_eq!(
            block_db_transaction
                .coin(&UtxoId::new(tx_id, 0))
                .unwrap()
                .amount,
            amount
        );
    }

    #[test]
    fn message_fails_when_spending_nonexistent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, _message) = make_tx_and_message(&mut rng, 0);

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = make_executor(&[]) // No messages in the db
            .execute_and_commit(ExecutionBlock::Production(block.clone().into()))
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            )
        ));

        // Produced block is valid
        make_executor(&[]) // No messages in the db
            .execute_and_commit(ExecutionBlock::Validation(block.clone()))
            .unwrap();

        // Invalidate block by returning back `tx` with not existing message
        block.transactions_mut().push(tx);
        let res = make_executor(&[]) // No messages in the db
            .execute_and_commit(ExecutionBlock::Validation(block));
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            ))
        ));
    }

    #[test]
    fn message_fails_when_spending_da_height_gt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 1); // Block has zero da_height

        let mut block = Block::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            mut block,
            ..
        } = make_executor(&[&message])
            .execute_and_commit(ExecutionBlock::Production(block.clone().into()))
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageSpendTooEarly(_)
            )
        ));

        // Produced block is valid
        make_executor(&[&message])
            .execute_and_commit(ExecutionBlock::Validation(block.clone()))
            .unwrap();

        // Invalidate block by return back `tx` with not ready message.
        block.transactions_mut().push(tx);
        let res = make_executor(&[&message])
            .execute_and_commit(ExecutionBlock::Validation(block));
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageSpendTooEarly(_)
            ))
        ));
    }

    #[test]
    fn message_fails_when_spending_already_spent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        // Create two transactions with the same message
        let (tx1, message) = make_tx_and_message(&mut rng, 0);
        let (mut tx2, _) = make_tx_and_message(&mut rng, 0);
        tx2.as_script_mut().unwrap().inputs_mut()[0] =
            tx1.as_script().unwrap().inputs()[0].clone();

        let mut block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1, tx2.clone()],
        };

        let exec = make_executor(&[&message]);
        let mut block_db_transaction = exec.database.transaction();
        let ExecutionData {
            skipped_transactions,
            ..
        } = exec
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Production(&mut block),
            )
            .unwrap();
        // One of two transactions is skipped.
        assert_eq!(skipped_transactions.len(), 1);
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &ExecutorError::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            )
        ));

        // Produced block is valid
        let exec = make_executor(&[&message]);
        let mut block_db_transaction = exec.database.transaction();
        exec.execute_transactions(
            &mut block_db_transaction,
            ExecutionType::Validation(&mut block),
        )
        .unwrap();

        // Invalidate block by return back `tx2` transaction skipped during production.
        block.transactions.push(tx2);
        let exec = make_executor(&[&message]);
        let mut block_db_transaction = exec.database.transaction();
        let res = exec.execute_transactions(
            &mut block_db_transaction,
            ExecutionType::Validation(&mut block),
        );
        assert!(matches!(
            res,
            Err(ExecutorError::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            ))
        ));
    }

    #[test]
    fn get_block_height_returns_current_executing_block() {
        let mut rng = StdRng::seed_from_u64(1234);

        // return current block height
        let script = vec![op::bhei(0x10), op::ret(0x10)];
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
        let block_tx_idx = rng.gen();

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: block_height.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.clone().into()],
        };

        // setup db with coin to spend
        let database = &mut &mut Database::default();
        let coin_input = &tx.inputs()[0];
        database
            .storage::<Coins>()
            .insert(
                coin_input.utxo_id().unwrap(),
                &CompressedCoin {
                    owner: *coin_input.input_owner().unwrap(),
                    amount: coin_input.amount().unwrap(),
                    asset_id: *coin_input.asset_id().unwrap(),
                    maturity: (coin_input.maturity().unwrap()).into(),
                    tx_pointer: TxPointer::new(0u32, block_tx_idx),
                },
            )
            .unwrap();

        // make executor with db
        let executor = Executor::test(
            database.clone(),
            Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        );

        executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        let receipts = database
            .storage::<Receipts>()
            .get(&tx.id())
            .unwrap()
            .unwrap();
        assert_eq!(block_height as u64, receipts[0].val().unwrap());
    }

    #[test]
    fn get_time_returns_current_executing_block_time() {
        let mut rng = StdRng::seed_from_u64(1234);

        // return current block height
        let script = vec![op::bhei(0x10), op::time(0x11, 0x10), op::ret(0x11)];
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
        let time = Tai64(rng.gen_range(1u32..u32::MAX) as u64);

        let block = PartialFuelBlock {
            header: PartialBlockHeader {
                consensus: ConsensusHeader {
                    height: block_height.into(),
                    time,
                    ..Default::default()
                },
                ..Default::default()
            },
            transactions: vec![tx.clone().into()],
        };

        // setup db with coin to spend
        let database = &mut &mut Database::default();
        let coin_input = &tx.inputs()[0];
        database
            .storage::<Coins>()
            .insert(
                coin_input.utxo_id().unwrap(),
                &CompressedCoin {
                    owner: *coin_input.input_owner().unwrap(),
                    amount: coin_input.amount().unwrap(),
                    asset_id: *coin_input.asset_id().unwrap(),
                    maturity: (coin_input.maturity().unwrap()).into(),
                    tx_pointer: TxPointer::default(),
                },
            )
            .unwrap();

        // make executor with db
        let executor = Executor::test(
            database.clone(),
            Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        );

        executor
            .execute_and_commit(ExecutionBlock::Production(block))
            .unwrap();

        let receipts = database
            .storage::<Receipts>()
            .get(&tx.id())
            .unwrap()
            .unwrap();

        assert_eq!(time.0, receipts[0].val().unwrap());
    }
}
