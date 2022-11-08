use crate::{
    database::{
        storage::{
            ContractsLatestUtxo,
            FuelBlocks,
            Receipts,
        },
        transaction::TransactionIndex,
        transactional::DatabaseTransaction,
        vm_database::VmDatabase,
        Database,
    },
    model::{
        BlockHeight,
        Coin,
        CoinStatus,
    },
    service::Config,
    tx_pool::TransactionStatus,
};
use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_storage,
        fuel_tx::{
            field::{
                Outputs,
                TxPointer as TxPointerField,
            },
            Address,
            AssetId,
            Bytes32,
            Checked,
            CheckedTransaction,
            CreateCheckedMetadata,
            Input,
            IntoChecked,
            Mint,
            Output,
            Receipt,
            ScriptCheckedMetadata,
            Transaction,
            TransactionFee,
            TxPointer,
            UniqueIdentifier,
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
        interpreter::CheckedMetadata,
        prelude::{
            field::Inputs,
            ExecutableTransaction,
            StorageInspect,
        },
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
        ExecutionResult,
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
        PartialFuelBlockHeader,
    },
};
use fuel_storage::{
    StorageAsMut,
    StorageAsRef,
};
use itertools::Itertools;
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
    skipped_transactions: Vec<(Transaction, Error)>,
}

impl ExecutorTrait for Executor {
    fn execute(&self, block: ExecutionBlock) -> Result<ExecutionResult, Error> {
        self.execute_inner(block, &self.database)
    }

    fn dry_run(
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

        let ExecutionResult {
            block,
            skipped_transactions,
        } = executor.execute_inner(block, temporary_db)?;

        // If one of the transactions fails, return an error.
        if let Some((_, err)) = skipped_transactions.into_iter().next() {
            return Err(err)
        }

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
    fn execute_inner(
        &self,
        block: ExecutionBlock,
        database: &Database,
    ) -> Result<ExecutionResult, Error> {
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
        let execution_data =
            self.execute_transactions(&mut block_db_transaction, block.as_mut())?;

        let ExecutionData {
            coinbase,
            message_ids,
            mut tx_status,
            skipped_transactions,
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

        // insert block into database
        block_db_transaction
            .deref_mut()
            .storage::<FuelBlocks>()
            .insert(&finalized_block_id.into(), &block.to_db_block())?;

        // Commit the database transaction.
        block_db_transaction.commit()?;

        // Get the complete fuel block.
        Ok(ExecutionResult {
            block: block.into_inner(),
            skipped_transactions,
        })
    }

    #[tracing::instrument(skip(self))]
    /// Execute all transactions on the fuel block.
    fn execute_transactions(
        &self,
        block_db_transaction: &mut DatabaseTransaction,
        block: ExecutionType<&mut PartialFuelBlock>,
    ) -> Result<ExecutionData, Error> {
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
                    return Err(Error::CoinbaseIsNotFirstTransaction)
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
        header: &PartialFuelBlockHeader,
        execution_data: &mut ExecutionData,
        execution_kind: ExecutionKind,
        tx_db_transaction: &mut DatabaseTransaction,
    ) -> Result<(), Error> {
        let tx_id = tx.id();
        // Throw a clear error if the transaction id is a duplicate
        if tx_db_transaction
            .deref_mut()
            .storage::<Transactions>()
            .contains_key(&tx_id)?
        {
            return Err(Error::TransactionIdCollision(tx_id))
        }

        let block_height: u32 = (*header.height()).into();
        // Wrap the transaction in the execution kind.
        let mut wrapped_tx: ExecutionTypes<&mut Transaction, &Transaction> =
            match execution_kind {
                ExecutionKind::Production => ExecutionTypes::Production(tx),
                ExecutionKind::Validation => ExecutionTypes::Validation(tx),
            };
        self.compute_contract_input_utxo_ids(&mut wrapped_tx, tx_db_transaction.deref())?;

        let checked_tx: CheckedTransaction = tx
            .clone()
            .into_checked_basic(
                block_height as Word,
                &self.config.chain_conf.transaction_parameters,
            )?
            .into();

        match checked_tx {
            CheckedTransaction::Script(script) => {
                *tx = self.execute_create_or_script(
                    idx,
                    script,
                    header,
                    execution_data,
                    tx_db_transaction,
                    execution_kind,
                )?
            }
            CheckedTransaction::Create(create) => {
                *tx = self.execute_create_or_script(
                    idx,
                    create,
                    header,
                    execution_data,
                    tx_db_transaction,
                    execution_kind,
                )?
            }
            CheckedTransaction::Mint(mint) => {
                // Right now, we only support `Mint` transactions for coinbase,
                // which are processed separately as a first transaction.
                //
                // All other `Mint` transactions are not allowed.
                let (mint, _): (Mint, _) = mint.into();
                return Err(Error::NotSupportedTransaction(Box::new(mint.into())))
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
    ) -> Result<(), Error> {
        let coinbase_id = coinbase_tx.id();
        self.persist_outputs(
            *block.header.height(),
            &coinbase_id,
            block_db_transaction,
            &[],
            coinbase_tx.outputs(),
        )?;
        execution_data.tx_status.insert(
            0,
            (
                coinbase_id,
                TransactionStatus::Success {
                    block_id: Default::default(),
                    time: *block.header.time(),
                    result: None,
                },
            ),
        );
        if block_db_transaction
            .deref_mut()
            .storage::<Transactions>()
            .insert(&coinbase_id, &coinbase_tx.into())?
            .is_some()
        {
            return Err(Error::TransactionIdCollision(coinbase_id))
        }
        Ok(())
    }

    fn check_coinbase(
        &self,
        block_height: Word,
        mint: Mint,
        expected_amount: Option<Word>,
    ) -> Result<Mint, Error> {
        let checked_mint = mint
            .into_checked(block_height, &self.config.chain_conf.transaction_parameters)?;

        if checked_mint.transaction().tx_pointer().tx_index() != 0 {
            return Err(Error::CoinbaseIsNotFirstTransaction)
        }

        if checked_mint.transaction().outputs().len() > 1 {
            return Err(Error::CoinbaseSeveralOutputs)
        }

        if let Some(Output::Coin {
            asset_id, amount, ..
        }) = checked_mint.transaction().outputs().first()
        {
            if asset_id != &AssetId::BASE {
                return Err(Error::CoinbaseOutputIsInvalid)
            }

            if let Some(expected_amount) = expected_amount {
                if expected_amount != *amount {
                    return Err(Error::CoinbaseAmountMismatch)
                }
            }
        } else {
            return Err(Error::CoinbaseOutputIsInvalid)
        }

        let (mint, _) = checked_mint.into();
        Ok(mint)
    }

    fn execute_create_or_script<Tx>(
        &self,
        idx: u16,
        checked_tx: Checked<Tx>,
        header: &PartialFuelBlockHeader,
        execution_data: &mut ExecutionData,
        tx_db_transaction: &mut DatabaseTransaction,
        execution_kind: ExecutionKind,
    ) -> Result<Transaction, Error>
    where
        Tx: ExecutableTransaction + PartialEq,
        <Tx as IntoChecked>::Metadata: Fee + CheckedMetadata + Clone,
    {
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

        // index owners of inputs and outputs with tx-id, regardless of validity (hence block_tx instead of tx_db)
        self.persist_owners_index(
            *header.height(),
            checked_tx.transaction(),
            &tx_id,
            idx,
            tx_db_transaction.deref_mut(),
        )?;

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
        );
        let vm_result: StateTransition<_> = vm
            .transact(checked_tx.clone())
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
            vm_result.tx().price(),
            vm_result.receipts(),
        )?;

        // Check or set the executed transaction.
        match execution_kind {
            ExecutionKind::Validation => {
                // ensure tx matches vm output exactly
                if vm_result.tx() != checked_tx.transaction() {
                    return Err(Error::InvalidTransactionOutcome {
                        transaction_id: tx_id,
                    })
                }
            }
            ExecutionKind::Production => {
                // malleate the block with the resultant tx from the vm
            }
        }

        let tx = vm_result.tx().clone().into();
        // Store tx into the block db transaction
        tx_db_transaction
            .deref_mut()
            .storage::<Transactions>()
            .insert(&tx_id, &tx)?;

        // change the spent status of the tx inputs
        self.spend_inputs(
            vm_result.tx(),
            tx_db_transaction.deref_mut(),
            *header.height(),
            self.config.utxo_validation,
        )?;

        // persist any outputs
        self.persist_outputs(
            *header.height(),
            &tx_id,
            tx_db_transaction.deref_mut(),
            vm_result.tx().inputs(),
            vm_result.tx().outputs(),
        )?;

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
                    Receipt::Revert { ra, .. } => Some(format!("Revert({})", ra)),
                    // Display PanicReason e.g. `OutOfGas`
                    Receipt::Panic { reason, .. } => Some(format!("{}", reason.reason())),
                    _ => None,
                })
                .unwrap_or_else(|| format!("{:?}", vm_result.state()));

            TransactionStatus::Failed {
                block_id: Default::default(),
                time: *header.time(),
                reason,
                result: Some(*vm_result.state()),
            }
        } else {
            // else tx was a success
            TransactionStatus::Success {
                block_id: Default::default(),
                time: *header.time(),
                result: Some(*vm_result.state()),
            }
        };

        // Update `execution_data` data only after all steps.
        execution_data.coinbase = execution_data
            .coinbase
            .checked_add(tx_fee)
            .ok_or(Error::FeeOverflow)?;
        // queue up status for this tx to be stored once block id is finalized.
        execution_data.tx_status.push((tx_id, status));
        execution_data
            .message_ids
            .extend(vm_result.receipts().iter().filter_map(|r| match r {
                Receipt::MessageOut { message_id, .. } => Some(*message_id),
                _ => None,
            }));

        Ok(tx)
    }

    fn verify_input_state<Tx: ExecutableTransaction>(
        &self,
        db: &Database,
        transaction: &Tx,
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
    pub fn verify_tx_predicates<Tx>(&self, tx: Checked<Tx>) -> Result<(), Error>
    where
        Tx: ExecutableTransaction,
        <Tx as IntoChecked>::Metadata: CheckedMetadata,
    {
        let id = tx.transaction().id();
        if !Interpreter::<PredicateStorage>::check_predicates(
            tx,
            self.config.chain_conf.transaction_parameters,
        ) {
            return Err(Error::TransactionValidity(
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
    fn spend_inputs<Tx>(
        &self,
        tx: &Tx,
        db: &mut Database,
        block_height: BlockHeight,
        utxo_validation: bool,
    ) -> Result<(), Error>
    where
        Tx: ExecutableTransaction,
    {
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
            ExecutionTypes::Production(tx) => {
                let inputs = match tx {
                    Transaction::Script(script) => script.inputs_mut(),
                    Transaction::Create(create) => create.inputs_mut(),
                    Transaction::Mint(_) => return Ok(()),
                };

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
            ExecutionTypes::Validation(ref tx) => {
                let inputs = match tx {
                    Transaction::Script(script) => script.inputs(),
                    Transaction::Create(create) => create.inputs(),
                    Transaction::Mint(_) => return Ok(()),
                };

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
        }
        Ok(())
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
                    hex::encode(&backtrace.memory()[..backtrace.registers()[REG_SP] as usize]), // print stack
                );
            }
        }
    }

    fn persist_outputs(
        &self,
        block_height: BlockHeight,
        tx_id: &Bytes32,
        db: &mut Database,
        inputs: &[Input],
        outputs: &[Output],
    ) -> Result<(), Error> {
        for (output_index, output) in outputs.iter().enumerate() {
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
                        inputs.get(*input_idx as usize)
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
        if db.storage::<Receipts>().insert(tx_id, receipts)?.is_some() {
            return Err(Error::OutputAlreadyExists)
        }
        Ok(())
    }

    /// Index the tx id by owner for all of the inputs and outputs
    fn persist_owners_index<Tx>(
        &self,
        block_height: BlockHeight,
        tx: &Tx,
        tx_id: &Bytes32,
        tx_idx: u16,
        db: &mut Database,
    ) -> Result<(), Error>
    where
        Tx: ExecutableTransaction,
    {
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

trait Fee {
    fn max_fee(&self) -> Word;

    fn min_fee(&self) -> Word;
}

impl Fee for ScriptCheckedMetadata {
    fn max_fee(&self) -> Word {
        self.fee.total()
    }

    fn min_fee(&self) -> Word {
        TransactionFee::min(&self.fee)
    }
}

impl Fee for CreateCheckedMetadata {
    fn max_fee(&self) -> Word {
        self.fee.total()
    }

    fn min_fee(&self) -> Word {
        TransactionFee::min(&self.fee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FuelBlock;
    use fuel_core_interfaces::{
        common::{
            fuel_asm::Opcode,
            fuel_crypto::SecretKey,
            fuel_tx,
            fuel_tx::{
                field::Outputs,
                Buildable,
                Chargeable,
                CheckError,
                ConsensusParameters,
                Create,
                Script,
                Transaction,
                TransactionBuilder,
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
            tai64::Tai64,
        },
        executor::ExecutionTypes,
        model::{
            CheckedMessage,
            DaBlockHeight,
            FuelConsensusHeader,
            Message,
            PartialFuelBlockHeader,
        },
    };
    use itertools::Itertools;
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };

    fn setup_executable_script() -> (Create, Script) {
        let mut rng = StdRng::seed_from_u64(2322);
        let asset_id: AssetId = rng.gen();
        let owner: Address = rng.gen();
        let input_amount = 1000;
        let variable_transfer_amount = 100;
        let coin_output_amount = 150;

        let (create, contract_id) = create_contract(
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
                    .into()
            })
            .collect_vec();

        let mut block = FuelBlock::default();
        *block.transactions_mut() = transactions;
        block
    }

    fn create_contract<R: Rng>(
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
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let verifier = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let block = test_block(10);

        let ExecutionResult {
            block,
            skipped_transactions,
        } = producer
            .execute(ExecutionTypes::Production(block.into()))
            .unwrap();

        let validation_result = verifier.execute(ExecutionTypes::Validation(block));
        assert!(validation_result.is_ok());
        assert!(skipped_transactions.is_empty());
    }

    // Ensure transaction commitment != default after execution
    #[test]
    fn executor_commits_transactions_to_block() {
        let producer = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };
        let block = test_block(10);
        let start_block = block.clone();

        let ExecutionResult {
            block,
            skipped_transactions,
        } = producer
            .execute(ExecutionBlock::Production(block.into()))
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
        use fuel_core_interfaces::common::{
            consts::REG_HP,
            fuel_asm::GTFArgs,
        };

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

            let mut producer = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
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

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![script.into(), invalid_duplicate_tx];

            let ExecutionResult {
                block,
                skipped_transactions,
            } = producer
                .execute(ExecutionBlock::Production(block.into()))
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

            let mut producer = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let recipient = [1u8; 32].into();
            producer.config.block_producer.coinbase_recipient = recipient;
            producer
                .config
                .chain_conf
                .transaction_parameters
                .gas_price_factor = gas_price_factor;

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![script.into()];

            let ExecutionResult {
                block: produced_block,
                skipped_transactions,
            } = producer
                .execute(ExecutionBlock::Production(block.into()))
                .unwrap();
            assert!(skipped_transactions.is_empty());
            let produced_txs = produced_block.transactions().to_vec();

            let validator = Executor {
                database: Default::default(),
                // Use the same config as block producer
                config: producer.config,
            };
            let ExecutionResult {
                block: validated_block,
                ..
            } = validator
                .execute(ExecutionBlock::Validation(produced_block))
                .unwrap();
            assert_eq!(validated_block.transactions(), produced_txs);
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
                        Opcode::MOVI(0x11, Address::LEN as Immediate18),
                        // Allocate 32 bytes on the heap.
                        Opcode::ALOC(0x11),
                        // Store the pointer to the beginning of the free memory into 
                        // register `0x10`. It requires shifting of `REG_HP` by 1 to point 
                        // on the free memory.
                        Opcode::ADDI(0x10, REG_HP, 1),
                        // Store `config_coinbase` `Address` into MEM[$0x10; 32].
                        Opcode::CB(0x10),
                        // Store the pointer on the beginning of script data into register `0x12`.
                        // Script data contains `expected_in_tx_coinbase` - 32 bytes of data.
                        Opcode::gtf(0x12, 0x00, GTFArgs::ScriptData),
                        // Compare retrieved `config_coinbase`(register `0x10`) with 
                        // passed `expected_in_tx_coinbase`(register `0x12`) where the length 
                        // of memory comparison is 32 bytes(register `0x11`) and store result into
                        // register `0x13`(1 - true, 0 - false). 
                        Opcode::MEQ(0x13, 0x10, 0x12, 0x11),
                        // Return the result of the comparison as a receipt.
                        Opcode::RET(0x13)
                    ], expected_in_tx_coinbase.to_vec() /* pass expected address as script data */)
                    .coin_input(AssetId::BASE, 1000)
                    .variable_output(Default::default())
                    .coin_output(AssetId::BASE, 1000)
                    .change_output(AssetId::BASE)
                    .build()
                    .transaction()
                    .clone();

                let mut producer = Executor {
                    database: Default::default(),
                    config: Config::local_node(),
                };
                producer.config.block_producer.coinbase_recipient = config_coinbase;

                let mut block = FuelBlock::default();
                *block.transactions_mut() = vec![script.clone().into()];

                assert!(producer
                    .execute(ExecutionBlock::Production(block.into()))
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

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![mint.into()];

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                Error::CoinbaseIsNotFirstTransaction
            ));
        }

        #[test]
        fn invalidate_block_height() {
            let mint = Transaction::mint(TxPointer::new(1, 0), vec![]);

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![mint.into()];

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(
                validation_err,
                Error::InvalidTransaction(
                    CheckError::TransactionMintIncorrectBlockHeight
                )
            ));
        }

        #[test]
        fn invalidate_zero_outputs() {
            let mint = Transaction::mint(TxPointer::new(0, 0), vec![]);

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![mint.into()];

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(validation_err, Error::CoinbaseOutputIsInvalid));
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

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![mint.into()];

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(validation_err, Error::CoinbaseSeveralOutputs));
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

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![mint.into()];

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(validation_err, Error::CoinbaseOutputIsInvalid));
        }

        #[test]
        fn invalidate_mismatch_amount() {
            let mint = Transaction::mint(
                TxPointer::new(0, 0),
                vec![Output::coin(Address::from([1u8; 32]), 123, AssetId::BASE)],
            );

            let mut block = FuelBlock::default();
            *block.transactions_mut() = vec![mint.into()];

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(validation_err, Error::CoinbaseAmountMismatch));
        }

        #[test]
        fn invalidate_more_than_one_mint_is_not_allowed() {
            let mut block = FuelBlock::default();
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

            let validator = Executor {
                database: Default::default(),
                config: Config::local_node(),
            };
            let validation_err = validator
                .execute(ExecutionBlock::Validation(block))
                .expect_err("Expected error because coinbase if invalid");
            assert!(matches!(validation_err, Error::NotSupportedTransaction(_)));
        }
    }

    // Ensure tx has at least one input to cover gas
    #[test]
    fn executor_invalidates_missing_gas_input() {
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
            &Error::InvalidTransaction(CheckError::InsufficientFeeAmount { expected, .. }) if expected == (gas_limit as f64 / factor).ceil() as u64
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
            Err(Error::InvalidTransaction(CheckError::InsufficientFeeAmount { expected, ..})) if expected == (gas_limit as f64 / factor).ceil() as u64
        ))
    }

    #[test]
    fn executor_invalidates_duplicate_tx_id() {
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
        assert!(matches!(produce_result, &Error::TransactionIdCollision(_)));

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
            Err(Error::TransactionIdCollision(_))
        ));
    }

    // invalidate a block if a tx input contains a previously used txo
    #[test]
    fn executor_invalidates_spent_inputs() {
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
        let tx: Transaction = Transaction::script(
            0,
            0,
            0,
            vec![],
            vec![],
            vec![input],
            vec![output],
            vec![Default::default()],
        )
        .into();

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
            config,
        };

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
            &Error::TransactionValidity(TransactionValidityError::CoinAlreadySpent(_))
        ));

        // Produced block is valid
        let mut block_db_transaction = verifier.database.transaction();
        verifier
            .execute_transactions(
                &mut block_db_transaction,
                ExecutionType::Validation(&mut block),
            )
            .unwrap();

        // Invalidate block by adding transaction with spent coin
        block.transactions.push(tx);
        let mut block_db_transaction = verifier.database.transaction();
        let verify_result = verifier.execute_transactions(
            &mut block_db_transaction,
            ExecutionType::Validation(&mut block),
        );
        assert!(matches!(
            verify_result,
            Err(Error::TransactionValidity(
                TransactionValidityError::CoinAlreadySpent(_)
            ))
        ));
    }

    // invalidate a block if a tx input doesn't exist
    #[test]
    fn executor_invalidates_missing_inputs() {
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
        .finalize_as_transaction();

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
            config,
        };

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
            &Error::TransactionValidity(TransactionValidityError::CoinDoesNotExist(_))
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
            Err(Error::TransactionValidity(
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

        let ExecutionResult { mut block, .. } = producer
            .execute(ExecutionBlock::Production(block.into()))
            .unwrap();

        // modify change amount
        if let Transaction::Script(script) = &mut block.transactions_mut()[1] {
            if let Output::Change { amount, .. } = &mut script.outputs_mut()[0] {
                *amount = fake_output_amount
            }
        }

        let verify_result = verifier.execute(ExecutionBlock::Validation(block));
        assert!(matches!(
            verify_result,
            Err(Error::InvalidTransactionOutcome { transaction_id }) if transaction_id == tx_id
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

        let ExecutionResult { mut block, .. } = producer
            .execute(ExecutionBlock::Production(block.into()))
            .unwrap();

        // randomize transaction commitment
        block.header_mut().application.generated.transactions_root = rng.gen();

        let verify_result = verifier.execute(ExecutionBlock::Validation(block));

        assert!(matches!(verify_result, Err(Error::InvalidTransactionRoot)))
    }

    // invalidate a block if a tx is missing at least one coin input
    #[test]
    fn executor_invalidates_missing_coin_input() {
        let tx: Transaction =
            TxBuilder::new(2322u64).build().transaction().clone().into();
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

        let ExecutionResult {
            skipped_transactions,
            ..
        } = executor.execute(ExecutionBlock::Production(block)).unwrap();

        let err = &skipped_transactions[0].1;
        // assert block failed to validate when transaction didn't contain any coin inputs
        assert!(matches!(
            err,
            &Error::TransactionValidity(TransactionValidityError::NoCoinOrMessageInput(id)) if id == tx_id
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
                &Coin {
                    owner: *first_input.input_owner().unwrap(),
                    amount: 100,
                    asset_id: AssetId::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: Default::default(),
                },
            )
            .unwrap();
        db.storage::<Coins>()
            .insert(
                &second_input.utxo_id().unwrap().clone(),
                &Coin {
                    owner: *second_input.input_owner().unwrap(),
                    amount: 100,
                    asset_id: AssetId::default(),
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: Default::default(),
                },
            )
            .unwrap();
        let executor = Executor {
            database: db.clone(),
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx1.into(), tx2.clone().into()],
        };

        // The first input should be `Unspent` before execution.
        let coin = db
            .storage::<Coins>()
            .get(first_input.utxo_id().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Unspent);
        // The second input should be `Unspent` before execution.
        let coin = db
            .storage::<Coins>()
            .get(second_input.utxo_id().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Unspent);

        let ExecutionResult {
            block,
            skipped_transactions,
        } = executor.execute(ExecutionBlock::Production(block)).unwrap();
        // `tx2` should be skipped.
        assert_eq!(block.transactions().len(), 2 /* coinbase and `tx1` */);
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(skipped_transactions[0].0.as_script(), Some(&tx2));

        // The first input should be spent by `tx1` after execution.
        let coin = db
            .storage::<Coins>()
            .get(first_input.utxo_id().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Spent);
        // The second input should be `Unspent` after execution.
        let coin = db
            .storage::<Coins>()
            .get(second_input.utxo_id().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Unspent);
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

        let executor = Executor {
            database: Default::default(),
            config: Config::local_node(),
        };

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
        } = executor.execute(ExecutionBlock::Production(block)).unwrap();
        assert_eq!(
            block.transactions().len(),
            3 // coinbase, `tx2` and `tx3`
        );
        assert_eq!(block.transactions()[1].id(), tx2.id());
        assert_eq!(block.transactions()[2].id(), tx3.id());
        // `tx1` should be skipped.
        assert_eq!(skipped_transactions.len(), 1);
        assert_eq!(skipped_transactions[0].0.as_script(), Some(&tx1));
        // TODO: Uncomment when https://github.com/FuelLabs/fuel-core/issues/544 ready
        // let tx2_index_in_the_block =
        //     block.transactions()[2].as_script().unwrap().inputs()[0]
        //         .tx_pointer()
        //         .unwrap()
        //         .tx_index();
        // assert_eq!(tx2_index_in_the_block, 1);
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
        let executor = Executor {
            database: db.clone(),
            config: Config::local_node(),
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        let ExecutionResult { block, .. } =
            executor.execute(ExecutionBlock::Production(block)).unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(
                block.transactions()[1].as_script().unwrap().inputs()[0]
                    .utxo_id()
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Spent);
    }

    #[test]
    fn input_coins_are_marked_as_spent_with_utxo_validation_enabled() {
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
            transactions: vec![tx.into()],
        };

        let ExecutionResult { block, .. } =
            executor.execute(ExecutionBlock::Production(block)).unwrap();

        // assert the tx coin is spent
        let coin = db
            .storage::<Coins>()
            .get(
                block.transactions()[1].as_script().unwrap().inputs()[0]
                    .utxo_id()
                    .unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(coin.status, CoinStatus::Spent);
        // assert block created from coin before spend is still intact (only a concern when utxo-validation is enabled)
        assert_eq!(coin.block_created, starting_block)
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
            .start_script(vec![Opcode::RET(1)], vec![])
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
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
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor {
            database: producer_view,
            config: Config::local_node(),
        };
        let ExecutionResult {
            block: second_block,
            ..
        } = producer
            .execute(ExecutionBlock::Production(second_block))
            .unwrap();

        let verifier = Executor {
            database: db,
            config: Config::local_node(),
        };
        let verify_result = verifier.execute(ExecutionBlock::Validation(second_block));
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
            .start_script(
                vec![Opcode::ADDI(0x10, REG_ZERO, 0), Opcode::RET(1)],
                vec![],
            )
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
            .start_script(
                vec![Opcode::ADDI(0x10, REG_ZERO, 1), Opcode::RET(1)],
                vec![],
            )
            .contract_input(contract_id)
            .contract_output(&contract_id)
            .build()
            .transaction()
            .clone()
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
            .unwrap();

        let producer_view = db.transaction().deref_mut().clone();
        let producer = Executor {
            database: producer_view,
            config: Config::local_node(),
        };

        let ExecutionResult {
            block: mut second_block,
            ..
        } = producer
            .execute(ExecutionBlock::Production(second_block))
            .unwrap();
        // Corrupt the utxo_id of the contract output
        if let Transaction::Script(script) = &mut second_block.transactions_mut()[1] {
            if let Input::Contract { utxo_id, .. } = &mut script.inputs_mut()[0] {
                // use a previously valid contract id which isn't the correct one for this block
                *utxo_id = UtxoId::new(tx_id, 0);
            }
        }

        let verifier = Executor {
            database: db,
            config: Config::local_node(),
        };
        let verify_result = verifier.execute(ExecutionBlock::Validation(second_block));

        assert!(matches!(
            verify_result,
            Err(Error::InvalidTransactionOutcome {
                transaction_id
            }) if transaction_id == tx_id
        ));
    }

    #[test]
    fn outputs_with_amount_are_included_utxo_set() {
        let (deploy, script) = setup_executable_script();
        let script_id = script.id();

        let database = &Database::default();
        let executor = Executor {
            database: database.clone(),
            config: Config::local_node(),
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![deploy.into(), script.into()],
        };

        let ExecutionResult { block, .. } =
            executor.execute(ExecutionBlock::Production(block)).unwrap();

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
        let executor = Executor {
            database: database.clone(),
            config: Config::local_node(),
        };

        let block = PartialFuelBlock {
            header: Default::default(),
            transactions: vec![tx],
        };

        executor.execute(ExecutionBlock::Production(block)).unwrap();

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

        (tx.into(), message.check())
    }

    /// Helper to build database and executor for some of the message tests
    fn make_executor(messages: &[&CheckedMessage]) -> Executor {
        let mut database = Database::default();
        let database_ref = &mut database;

        for message in messages {
            database_ref
                .storage::<Messages>()
                .insert(message.id(), message.as_ref())
                .unwrap();
        }

        Executor {
            database,
            config: Config {
                utxo_validation: true,
                ..Config::local_node()
            },
        }
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
            .execute(ExecutionBlock::Production(block))
            .expect("block execution failed unexpectedly");

        make_executor(&[&message])
            .execute(ExecutionBlock::Validation(block))
            .expect("block validation failed unexpectedly");
    }

    #[test]
    fn message_fails_when_spending_nonexistent_message_id() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, _message) = make_tx_and_message(&mut rng, 0);

        let mut block = FuelBlock::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            mut block,
        } = make_executor(&[]) // No messages in the db
            .execute(ExecutionBlock::Production(block.clone().into()))
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &Error::TransactionValidity(TransactionValidityError::MessageDoesNotExist(_))
        ));

        // Produced block is valid
        make_executor(&[]) // No messages in the db
            .execute(ExecutionBlock::Validation(block.clone()))
            .unwrap();

        // Invalidate block by returning back `tx` with not existing message
        block.transactions_mut().push(tx);
        let res = make_executor(&[]) // No messages in the db
            .execute(ExecutionBlock::Validation(block));
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageDoesNotExist(_)
            ))
        ));
    }

    #[test]
    fn message_fails_when_spending_da_height_gt_block_da_height() {
        let mut rng = StdRng::seed_from_u64(2322);

        let (tx, message) = make_tx_and_message(&mut rng, 1); // Block has zero da_height

        let mut block = FuelBlock::default();
        *block.transactions_mut() = vec![tx.clone()];

        let ExecutionResult {
            skipped_transactions,
            mut block,
        } = make_executor(&[&message])
            .execute(ExecutionBlock::Production(block.clone().into()))
            .unwrap();
        let err = &skipped_transactions[0].1;
        assert!(matches!(
            err,
            &Error::TransactionValidity(TransactionValidityError::MessageSpendTooEarly(
                _
            ))
        ));

        // Produced block is valid
        make_executor(&[&message])
            .execute(ExecutionBlock::Validation(block.clone()))
            .unwrap();

        // Invalidate block by return back `tx` with not ready message.
        block.transactions_mut().push(tx);
        let res = make_executor(&[&message]).execute(ExecutionBlock::Validation(block));
        assert!(matches!(
            res,
            Err(Error::TransactionValidity(
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
            &Error::TransactionValidity(TransactionValidityError::MessageAlreadySpent(_))
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
            Err(Error::TransactionValidity(
                TransactionValidityError::MessageAlreadySpent(_)
            ))
        ));
    }

    #[test]
    fn get_block_height_returns_current_executing_block() {
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
            transactions: vec![tx.clone().into()],
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

        executor.execute(ExecutionBlock::Production(block)).unwrap();

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
        let time = Tai64(rng.gen_range(1u32..u32::MAX) as u64);

        let block = PartialFuelBlock {
            header: PartialFuelBlockHeader {
                consensus: FuelConsensusHeader {
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

        executor.execute(ExecutionBlock::Production(block)).unwrap();

        let receipts = database
            .storage::<Receipts>()
            .get(&tx.id())
            .unwrap()
            .unwrap();

        assert_eq!(time.0, receipts[0].val().unwrap());
    }
}
