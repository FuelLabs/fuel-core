use crate::{
    ports::{
        ExecutorDatabaseTrait,
        MaybeCheckedTransaction,
        RelayerPort,
        TransactionsSource,
    },
    refs::ContractRef,
    Config,
};
use block_component::*;
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsInfo,
        ContractsLatestUtxo,
        FuelBlocks,
        Messages,
        ProcessedTransactions,
        SpentMessages,
    },
    transactional::{
        AtomicView,
        StorageTransaction,
        Transactional,
    },
    vm_storage::VmStorage,
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        header::PartialBlockHeader,
        primitives::DaBlockHeight,
    },
    entities::{
        coins::coin::{
            CompressedCoin,
            CompressedCoinV1,
        },
        contract::ContractUtxoInfo,
    },
    fuel_asm::{
        RegId,
        Word,
    },
    fuel_tx::{
        field::{
            InputContract,
            MintAmount,
            MintAssetId,
            OutputContract,
            TxPointer as TxPointerField,
        },
        input,
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
        output,
        Address,
        AssetId,
        Bytes32,
        Cacheable,
        Chargeable,
        Input,
        Mint,
        Output,
        Receipt,
        Transaction,
        TxId,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        ContractId,
        MessageId,
    },
    fuel_vm,
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            CheckPredicates,
            Checked,
            CheckedTransaction,
            Checks,
            CreateCheckedMetadata,
            IntoChecked,
            ScriptCheckedMetadata,
        },
        interpreter::{
            CheckedMetadata,
            ExecutableTransaction,
            InterpreterParams,
        },
        state::StateTransition,
        Backtrace as FuelBacktrace,
        Interpreter,
        InterpreterError,
    },
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            Event as ExecutorEvent,
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
        relayer::Event,
    },
};
use parking_lot::Mutex as ParkingMutex;
use std::{
    borrow::Cow,
    sync::Arc,
};
use tracing::{
    debug,
    warn,
};

pub type ExecutionBlockWithSource<TxSource> = ExecutionTypes<Components<TxSource>, Block>;

pub struct OnceTransactionsSource {
    transactions: ParkingMutex<Vec<MaybeCheckedTransaction>>,
}

impl OnceTransactionsSource {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self {
            transactions: ParkingMutex::new(
                transactions
                    .into_iter()
                    .map(MaybeCheckedTransaction::Transaction)
                    .collect(),
            ),
        }
    }
}

impl TransactionsSource for OnceTransactionsSource {
    fn next(&self, _: u64) -> Vec<MaybeCheckedTransaction> {
        let mut lock = self.transactions.lock();
        core::mem::take(lock.as_mut())
    }
}

/// The executor is used for block production and validation of the blocks.
#[derive(Clone, Debug)]
pub struct Executor<D, R> {
    pub database_view_provider: D,
    pub relayer_view_provider: R,
    pub config: Arc<Config>,
}

impl<D, R, View> Executor<D, R>
where
    R: AtomicView<Height = DaBlockHeight>,
    R::View: RelayerPort,
    D: AtomicView<View = View, Height = BlockHeight>,
    D::View: ExecutorDatabaseTrait<View>,
{
    #[cfg(any(test, feature = "test-helpers"))]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn execute_and_commit(
        &self,
        block: fuel_core_types::services::executor::ExecutionBlock,
        options: ExecutionOptions,
    ) -> ExecutorResult<ExecutionResult> {
        let executor = ExecutionInstance {
            database: self.database_view_provider.latest_view(),
            relayer: self.relayer_view_provider.latest_view(),
            config: self.config.clone(),
            options,
        };
        executor.execute_and_commit(block)
    }

    /// Executes the partial block and returns `ExecutionData` as a result.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn execute_block<TxSource>(
        &self,
        block: ExecutionType<PartialBlockComponent<TxSource>>,
        options: ExecutionOptions,
    ) -> ExecutorResult<ExecutionData>
    where
        TxSource: TransactionsSource,
    {
        let executor = ExecutionInstance {
            database: self.database_view_provider.latest_view(),
            relayer: self.relayer_view_provider.latest_view(),
            config: self.config.clone(),
            options,
        };
        let mut block_transaction = executor.database.transaction();
        executor.execute_block(block_transaction.as_mut(), block)
    }

    pub fn execute_without_commit<TxSource>(
        &self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<View>>>
    where
        TxSource: TransactionsSource,
    {
        let executor = ExecutionInstance {
            database: self.database_view_provider.latest_view(),
            relayer: self.relayer_view_provider.latest_view(),
            config: self.config.clone(),
            options: self.config.as_ref().into(),
        };
        executor.execute_inner(block)
    }

    pub fn dry_run(
        &self,
        component: Components<Vec<Transaction>>,
        utxo_validation: Option<bool>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        // fallback to service config value if no utxo_validation override is provided
        let utxo_validation =
            utxo_validation.unwrap_or(self.config.utxo_validation_default);

        let options = ExecutionOptions { utxo_validation };

        let executor = ExecutionInstance {
            database: self.database_view_provider.latest_view(),
            relayer: self.relayer_view_provider.latest_view(),
            config: self.config.clone(),
            options,
        };
        executor.dry_run(component)
    }
}

/// Data that is generated after executing all transactions.
#[derive(Default)]
pub struct ExecutionData {
    coinbase: u64,
    used_gas: u64,
    tx_count: u16,
    found_mint: bool,
    message_ids: Vec<MessageId>,
    tx_status: Vec<TransactionExecutionStatus>,
    events: Vec<ExecutorEvent>,
    pub skipped_transactions: Vec<(TxId, ExecutorError)>,
}

/// Per-block execution options
#[derive(Copy, Clone, Default, Debug)]
pub struct ExecutionOptions {
    /// UTXO Validation flag, when disabled the executor skips signature and UTXO existence checks
    pub utxo_validation: bool,
}

impl From<&Config> for ExecutionOptions {
    fn from(value: &Config) -> Self {
        Self {
            utxo_validation: value.utxo_validation_default,
        }
    }
}

/// The executor instance performs block production and validation. Given a block, it will execute all
/// the transactions contained in the block and persist changes to the underlying database as needed.
/// In production mode, block fields like transaction commitments are set based on the executed txs.
/// In validation mode, the processed block commitments are compared with the proposed block.
#[derive(Clone, Debug)]
struct ExecutionInstance<R, D> {
    pub relayer: R,
    pub database: D,
    pub config: Arc<Config>,
    pub options: ExecutionOptions,
}

impl<R, D> ExecutionInstance<R, D>
where
    R: RelayerPort,
    D: ExecutorDatabaseTrait<D>,
{
    #[cfg(any(test, feature = "test-helpers"))]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    fn execute_and_commit(
        self,
        block: fuel_core_types::services::executor::ExecutionBlock,
    ) -> ExecutorResult<ExecutionResult> {
        let component = match block {
            ExecutionTypes::DryRun(_) => {
                panic!("It is not possible to commit the dry run result");
            }
            ExecutionTypes::Production(block) => ExecutionTypes::Production(Components {
                header_to_produce: block.header,
                transactions_source: OnceTransactionsSource::new(block.transactions),
                gas_limit: u64::MAX,
            }),
            ExecutionTypes::Validation(block) => ExecutionTypes::Validation(block),
        };

        let (result, db_transaction) = self.execute_without_commit(component)?.into();
        db_transaction.commit()?;
        Ok(result)
    }
}

impl<R, D> ExecutionInstance<R, D>
where
    R: RelayerPort,
    D: ExecutorDatabaseTrait<D>,
{
    pub fn execute_without_commit<TxSource>(
        self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<D>>>
    where
        TxSource: TransactionsSource,
    {
        self.execute_inner(block)
    }

    pub fn dry_run(
        self,
        component: Components<Vec<Transaction>>,
    ) -> ExecutorResult<Vec<TransactionExecutionStatus>> {
        let component = Components {
            header_to_produce: component.header_to_produce,
            transactions_source: OnceTransactionsSource::new(
                component.transactions_source,
            ),
            gas_limit: component.gas_limit,
        };

        let (
            ExecutionResult {
                skipped_transactions,
                tx_status,
                ..
            },
            _temporary_db,
        ) = self
            .execute_without_commit(ExecutionTypes::DryRun(component))?
            .into();

        // If one of the transactions fails, return an error.
        if let Some((_, err)) = skipped_transactions.into_iter().next() {
            return Err(err)
        }

        Ok(tx_status)
        // drop `_temporary_db` without committing to avoid altering state.
    }
}

// TODO: Make this module private after moving unit tests from `fuel-core` here.
pub mod block_component {
    use super::*;

    pub struct PartialBlockComponent<'a, TxSource> {
        pub empty_block: &'a mut PartialFuelBlock,
        pub transactions_source: TxSource,
        pub gas_limit: u64,
        /// The private marker to allow creation of the type only by constructor.
        _marker: core::marker::PhantomData<()>,
    }

    impl<'a> PartialBlockComponent<'a, OnceTransactionsSource> {
        pub fn from_partial_block(block: &'a mut PartialFuelBlock) -> Self {
            let transaction = core::mem::take(&mut block.transactions);
            Self {
                empty_block: block,
                transactions_source: OnceTransactionsSource::new(transaction),
                gas_limit: u64::MAX,
                _marker: Default::default(),
            }
        }
    }

    impl<'a, TxSource> PartialBlockComponent<'a, TxSource> {
        pub fn from_component(
            block: &'a mut PartialFuelBlock,
            transactions_source: TxSource,
            gas_limit: u64,
        ) -> Self {
            debug_assert!(block.transactions.is_empty());
            PartialBlockComponent {
                empty_block: block,
                transactions_source,
                gas_limit,
                _marker: Default::default(),
            }
        }
    }
}

impl<R, D> ExecutionInstance<R, D>
where
    R: RelayerPort,
    D: ExecutorDatabaseTrait<D>,
{
    #[tracing::instrument(skip_all)]
    fn execute_inner<TxSource>(
        self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<D>>>
    where
        TxSource: TransactionsSource,
    {
        // Compute the block id before execution if there is one.
        let pre_exec_block_id = block.id();

        // If there is full fuel block for validation then map it into
        // a partial header.
        let block = block.map_v(PartialFuelBlock::from);

        // Create a new storage transaction.
        let mut block_st_transaction = self.database.transaction();

        let (block, execution_data) = match block {
            ExecutionTypes::DryRun(component) => {
                let mut block =
                    PartialFuelBlock::new(component.header_to_produce, vec![]);
                let component = PartialBlockComponent::from_component(
                    &mut block,
                    component.transactions_source,
                    component.gas_limit,
                );

                let execution_data = self.execute_block(
                    block_st_transaction.as_mut(),
                    ExecutionType::DryRun(component),
                )?;
                (block, execution_data)
            }
            ExecutionTypes::Production(component) => {
                let mut block =
                    PartialFuelBlock::new(component.header_to_produce, vec![]);
                let component = PartialBlockComponent::from_component(
                    &mut block,
                    component.transactions_source,
                    component.gas_limit,
                );

                let execution_data = self.execute_block(
                    block_st_transaction.as_mut(),
                    ExecutionType::Production(component),
                )?;
                (block, execution_data)
            }
            ExecutionTypes::Validation(mut block) => {
                let component = PartialBlockComponent::from_partial_block(&mut block);
                let execution_data = self.execute_block(
                    block_st_transaction.as_mut(),
                    ExecutionType::Validation(component),
                )?;
                (block, execution_data)
            }
        };

        let ExecutionData {
            coinbase,
            used_gas,
            message_ids,
            tx_status,
            skipped_transactions,
            events,
            ..
        } = execution_data;

        // Now that the transactions have been executed, generate the full header.

        let block = block.generate(&message_ids[..]);

        let finalized_block_id = block.id();

        debug!(
            "Block {:#x} fees: {} gas: {}",
            pre_exec_block_id.unwrap_or(finalized_block_id),
            coinbase,
            used_gas
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
            events,
        };

        // Get the complete fuel block.
        Ok(UncommittedResult::new(result, block_st_transaction))
    }

    #[tracing::instrument(skip_all)]
    /// Execute the fuel block with all transactions.
    fn execute_block<TxSource>(
        &self,
        block_st_transaction: &mut D,
        block: ExecutionType<PartialBlockComponent<TxSource>>,
    ) -> ExecutorResult<ExecutionData>
    where
        TxSource: TransactionsSource,
    {
        let mut data = ExecutionData {
            coinbase: 0,
            used_gas: 0,
            tx_count: 0,
            found_mint: false,
            message_ids: Vec::new(),
            tx_status: Vec::new(),
            events: Vec::new(),
            skipped_transactions: Vec::new(),
        };
        let execution_data = &mut data;

        // Split out the execution kind and partial block.
        let (execution_kind, component) = block.split();
        let block = component.empty_block;
        let source = component.transactions_source;
        let mut remaining_gas_limit = component.gas_limit;
        let block_height = *block.header.height();

        if self.relayer.enabled() {
            self.process_da(block_st_transaction, &block.header, execution_data)?;
        }

        // ALl transactions should be in the `TxSource`.
        // We use `block.transactions` to store executed transactions.
        debug_assert!(block.transactions.is_empty());
        let mut iter = source.next(remaining_gas_limit).into_iter().peekable();

        let mut execute_transaction = |execution_data: &mut ExecutionData,
                                       tx: MaybeCheckedTransaction|
         -> ExecutorResult<()> {
            let tx_count = execution_data.tx_count;
            let tx = {
                let mut tx_st_transaction = block_st_transaction.transaction();
                let tx_id = tx.id(&self.config.consensus_parameters.chain_id);
                let result = self.execute_transaction(
                    tx,
                    &tx_id,
                    &block.header,
                    execution_data,
                    execution_kind,
                    &mut tx_st_transaction,
                );

                let tx = match result {
                    Err(err) => {
                        return match execution_kind {
                            ExecutionKind::Production => {
                                // If, during block production, we get an invalid transaction,
                                // remove it from the block and continue block creation. An invalid
                                // transaction means that the caller didn't validate it first, so
                                // maybe something is wrong with validation rules in the `TxPool`
                                // (or in another place that should validate it). Or we forgot to
                                // clean up some dependent/conflict transactions. But it definitely
                                // means that something went wrong, and we must fix it.
                                execution_data.skipped_transactions.push((tx_id, err));
                                Ok(())
                            }
                            ExecutionKind::DryRun | ExecutionKind::Validation => Err(err),
                        }
                    }
                    Ok(tx) => tx,
                };

                if let Err(err) = tx_st_transaction.commit() {
                    return Err(err.into())
                }
                tx
            };

            block.transactions.push(tx);
            execution_data.tx_count = tx_count
                .checked_add(1)
                .ok_or(ExecutorError::TooManyTransactions)?;

            Ok(())
        };

        while iter.peek().is_some() {
            for transaction in iter {
                execute_transaction(&mut *execution_data, transaction)?;
            }

            remaining_gas_limit =
                component.gas_limit.saturating_sub(execution_data.used_gas);

            iter = source.next(remaining_gas_limit).into_iter().peekable();
        }

        // After the execution of all transactions in production mode, we can set the final fee.
        if execution_kind == ExecutionKind::Production {
            let amount_to_mint = if self.config.coinbase_recipient != ContractId::zeroed()
            {
                execution_data.coinbase
            } else {
                0
            };

            let coinbase_tx = Transaction::mint(
                TxPointer::new(block_height, execution_data.tx_count),
                input::contract::Contract {
                    utxo_id: UtxoId::new(Bytes32::zeroed(), 0),
                    balance_root: Bytes32::zeroed(),
                    state_root: Bytes32::zeroed(),
                    tx_pointer: TxPointer::new(BlockHeight::new(0), 0),
                    contract_id: self.config.coinbase_recipient,
                },
                output::contract::Contract {
                    input_index: 0,
                    balance_root: Bytes32::zeroed(),
                    state_root: Bytes32::zeroed(),
                },
                amount_to_mint,
                self.config.consensus_parameters.base_asset_id,
            );

            execute_transaction(
                execution_data,
                MaybeCheckedTransaction::Transaction(coinbase_tx.into()),
            )?;
        }

        if execution_kind != ExecutionKind::DryRun && !data.found_mint {
            return Err(ExecutorError::MintMissing)
        }

        Ok(data)
    }

    fn process_da(
        &self,
        block_st_transaction: &mut D,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
    ) -> ExecutorResult<()> {
        let block_height = *header.height();
        let prev_block_height = block_height
            .pred()
            .ok_or(ExecutorError::ExecutingGenesisBlock)?;

        let prev_block_header = block_st_transaction
            .storage::<FuelBlocks>()
            .get(&prev_block_height)?
            .ok_or(ExecutorError::PreviousBlockIsNotFound)?;
        let previous_da_height = prev_block_header.header().da_height;
        let Some(next_unprocessed_da_height) = previous_da_height.0.checked_add(1) else {
            return Err(ExecutorError::DaHeightExceededItsLimit)
        };

        for da_height in next_unprocessed_da_height..=header.da_height.0 {
            let da_height = da_height.into();
            let events = self
                .relayer
                .get_events(&da_height)
                .map_err(|err| ExecutorError::RelayerError(err.into()))?;
            for event in events {
                match event {
                    Event::Message(message) => {
                        if message.da_height() != da_height {
                            return Err(ExecutorError::RelayerGivesIncorrectMessages)
                        }
                        block_st_transaction
                            .storage::<Messages>()
                            .insert(message.nonce(), &message)?;
                        execution_data
                            .events
                            .push(ExecutorEvent::MessageImported(message));
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_transaction(
        &self,
        tx: MaybeCheckedTransaction,
        tx_id: &TxId,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
        execution_kind: ExecutionKind,
        tx_st_transaction: &mut StorageTransaction<D>,
    ) -> ExecutorResult<Transaction> {
        if execution_data.found_mint {
            return Err(ExecutorError::MintIsNotLastTransaction)
        }

        // Throw a clear error if the transaction id is a duplicate
        if tx_st_transaction
            .as_ref()
            .storage::<ProcessedTransactions>()
            .contains_key(tx_id)?
        {
            return Err(ExecutorError::TransactionIdCollision(*tx_id))
        }

        let block_height = *header.height();
        let checked_tx = match tx {
            MaybeCheckedTransaction::Transaction(tx) => tx
                .into_checked_basic(block_height, &self.config.consensus_parameters)?
                .into(),
            MaybeCheckedTransaction::CheckedTransaction(checked_tx) => checked_tx,
        };

        match checked_tx {
            CheckedTransaction::Script(script) => self.execute_create_or_script(
                script,
                header,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
            CheckedTransaction::Create(create) => self.execute_create_or_script(
                create,
                header,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
            CheckedTransaction::Mint(mint) => self.execute_mint(
                mint,
                header,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
        }
    }

    fn execute_mint(
        &self,
        checked_mint: Checked<Mint>,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
        block_st_transaction: &mut StorageTransaction<D>,
        execution_kind: ExecutionKind,
    ) -> ExecutorResult<Transaction> {
        execution_data.found_mint = true;

        if checked_mint.transaction().tx_pointer().tx_index() != execution_data.tx_count {
            return Err(ExecutorError::MintHasUnexpectedIndex)
        }

        let coinbase_id = checked_mint.id();
        let (mut mint, _) = checked_mint.into();

        fn verify_mint_for_empty_contract(mint: &Mint) -> ExecutorResult<()> {
            if *mint.mint_amount() != 0 {
                return Err(ExecutorError::CoinbaseAmountMismatch)
            }

            let input = input::contract::Contract {
                utxo_id: UtxoId::new(Bytes32::zeroed(), 0),
                balance_root: Bytes32::zeroed(),
                state_root: Bytes32::zeroed(),
                tx_pointer: TxPointer::new(BlockHeight::new(0), 0),
                contract_id: ContractId::zeroed(),
            };
            let output = output::contract::Contract {
                input_index: 0,
                balance_root: Bytes32::zeroed(),
                state_root: Bytes32::zeroed(),
            };
            if mint.input_contract() != &input || mint.output_contract() != &output {
                return Err(ExecutorError::MintMismatch)
            }
            Ok(())
        }

        if mint.input_contract().contract_id == ContractId::zeroed() {
            verify_mint_for_empty_contract(&mint)?;
        } else {
            if *mint.mint_amount() != execution_data.coinbase {
                return Err(ExecutorError::CoinbaseAmountMismatch)
            }

            let block_height = *header.height();

            let input = mint.input_contract().clone();
            let output = *mint.output_contract();
            let mut inputs = [Input::Contract(input)];
            let mut outputs = [Output::Contract(output)];

            if self.options.utxo_validation {
                // validate utxos exist
                self.verify_input_state(
                    block_st_transaction.as_ref(),
                    inputs.as_mut_slice(),
                    block_height,
                    header.da_height,
                )?;
            }

            self.compute_inputs(
                match execution_kind {
                    ExecutionKind::DryRun => {
                        ExecutionTypes::DryRun(inputs.as_mut_slice())
                    }
                    ExecutionKind::Production => {
                        ExecutionTypes::Production(inputs.as_mut_slice())
                    }
                    ExecutionKind::Validation => {
                        ExecutionTypes::Validation(inputs.as_slice())
                    }
                },
                coinbase_id,
                block_st_transaction.as_mut(),
            )?;

            let mut sub_block_db_commit = block_st_transaction.transaction();

            let mut vm_db = VmStorage::new(
                sub_block_db_commit.as_mut(),
                &header.consensus,
                self.config.coinbase_recipient,
            );

            fuel_vm::interpreter::contract::balance_increase(
                &mut vm_db,
                &mint.input_contract().contract_id,
                mint.mint_asset_id(),
                *mint.mint_amount(),
            )
            .map_err(|e| anyhow::anyhow!(format!("{e}")))
            .map_err(ExecutorError::CoinbaseCannotIncreaseBalance)?;
            sub_block_db_commit.commit()?;

            self.persist_output_utxos(
                block_height,
                execution_data,
                &coinbase_id,
                block_st_transaction.as_mut(),
                inputs.as_slice(),
                outputs.as_slice(),
            )?;
            self.compute_not_utxo_outputs(
                match execution_kind {
                    ExecutionKind::DryRun => ExecutionTypes::DryRun((
                        outputs.as_mut_slice(),
                        inputs.as_slice(),
                    )),
                    ExecutionKind::Production => ExecutionTypes::Production((
                        outputs.as_mut_slice(),
                        inputs.as_slice(),
                    )),
                    ExecutionKind::Validation => ExecutionTypes::Validation((
                        outputs.as_slice(),
                        inputs.as_slice(),
                    )),
                },
                coinbase_id,
                block_st_transaction.as_mut(),
            )?;
            let Input::Contract(input) = core::mem::take(&mut inputs[0]) else {
                unreachable!()
            };
            let Output::Contract(output) = outputs[0] else {
                unreachable!()
            };

            if execution_kind == ExecutionKind::Validation {
                if mint.input_contract() != &input || mint.output_contract() != &output {
                    return Err(ExecutorError::MintMismatch)
                }
            } else {
                *mint.input_contract_mut() = input;
                *mint.output_contract_mut() = output;
            }
        }

        let tx = mint.into();

        execution_data.tx_status.push(TransactionExecutionStatus {
            id: coinbase_id,
            result: TransactionExecutionResult::Success {
                result: None,
                receipts: vec![],
            },
        });

        if block_st_transaction
            .as_mut()
            .storage::<ProcessedTransactions>()
            .insert(&coinbase_id, &())?
            .is_some()
        {
            return Err(ExecutorError::TransactionIdCollision(coinbase_id))
        }
        Ok(tx)
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_create_or_script<Tx>(
        &self,
        mut checked_tx: Checked<Tx>,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
        tx_st_transaction: &mut StorageTransaction<D>,
        execution_kind: ExecutionKind,
    ) -> ExecutorResult<Transaction>
    where
        Tx: ExecutableTransaction + PartialEq + Cacheable + Send + Sync + 'static,
        <Tx as IntoChecked>::Metadata: Fee + CheckedMetadata + Clone + Send + Sync,
    {
        let tx_id = checked_tx.id();
        let max_fee = checked_tx.metadata().max_fee();

        if self.options.utxo_validation {
            checked_tx = checked_tx
                .check_predicates(&CheckPredicateParams::from(
                    &self.config.consensus_parameters,
                ))
                .map_err(|_| {
                    ExecutorError::TransactionValidity(
                        TransactionValidityError::InvalidPredicate(tx_id),
                    )
                })?;
            debug_assert!(checked_tx.checks().contains(Checks::Predicates));

            // validate utxos exist and maturity is properly set
            self.verify_input_state(
                tx_st_transaction.as_ref(),
                checked_tx.transaction().inputs(),
                *header.height(),
                header.da_height,
            )?;
            // validate transaction signature
            checked_tx = checked_tx
                .check_signatures(&self.config.consensus_parameters.chain_id)
                .map_err(TransactionValidityError::from)?;
            debug_assert!(checked_tx.checks().contains(Checks::Signatures));
        }

        // execute transaction
        // setup database view that only lives for the duration of vm execution
        let mut sub_block_db_commit = tx_st_transaction.transaction();
        let sub_db_view = sub_block_db_commit.as_mut();

        // execution vm
        let vm_db = VmStorage::new(
            sub_db_view.clone(),
            &header.consensus,
            self.config.coinbase_recipient,
        );

        let mut vm = Interpreter::with_storage(
            vm_db,
            InterpreterParams::from(&self.config.consensus_parameters),
        );
        let vm_result: StateTransition<_> = vm
            .transact(checked_tx.clone())
            .map_err(|error| ExecutorError::VmExecution {
                error: InterpreterError::Storage(anyhow::anyhow!(format!("{error:?}"))),
                transaction_id: tx_id,
            })?
            .into();
        let reverted = vm_result.should_revert();

        let (state, mut tx, receipts) = vm_result.into_inner();
        #[cfg(debug_assertions)]
        {
            tx.precompute(&self.config.consensus_parameters.chain_id)?;
            debug_assert_eq!(tx.id(&self.config.consensus_parameters.chain_id), tx_id);
        }

        // TODO: We need to call this function before `vm.transact` but we can't do that because of
        //  `Checked<Transaction>` immutability requirements. So we do it here after its execution for now.
        //  But it should be fixed in the future.
        //  https://github.com/FuelLabs/fuel-vm/issues/651
        self.compute_inputs(
            match execution_kind {
                ExecutionKind::DryRun => ExecutionTypes::DryRun(tx.inputs_mut()),
                ExecutionKind::Production => ExecutionTypes::Production(tx.inputs_mut()),
                ExecutionKind::Validation => ExecutionTypes::Validation(tx.inputs()),
            },
            tx_id,
            tx_st_transaction.as_mut(),
        )?;

        // only commit state changes if execution was a success
        if !reverted {
            sub_block_db_commit.commit()?;
        }

        // update block commitment
        let (used_gas, tx_fee) = self.total_fee_paid(&tx, max_fee, &receipts)?;

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
            ExecutionKind::DryRun | ExecutionKind::Production => {
                // malleate the block with the resultant tx from the vm
            }
        }

        // change the spent status of the tx inputs
        self.spend_input_utxos(
            tx.inputs(),
            tx_st_transaction.as_mut(),
            reverted,
            execution_data,
        )?;

        // Persist utxos first and after calculate the not utxo outputs
        self.persist_output_utxos(
            *header.height(),
            execution_data,
            &tx_id,
            tx_st_transaction.as_mut(),
            tx.inputs(),
            tx.outputs(),
        )?;
        // TODO: Inputs, in most cases, are heavier than outputs, so cloning them, but we
        //  need to avoid cloning in the future.
        let mut outputs = tx.outputs().clone();
        self.compute_not_utxo_outputs(
            match execution_kind {
                ExecutionKind::DryRun => {
                    ExecutionTypes::DryRun((&mut outputs, tx.inputs()))
                }
                ExecutionKind::Production => {
                    ExecutionTypes::Production((&mut outputs, tx.inputs()))
                }
                ExecutionKind::Validation => {
                    ExecutionTypes::Validation((&outputs, tx.inputs()))
                }
            },
            tx_id,
            tx_st_transaction.as_mut(),
        )?;
        *tx.outputs_mut() = outputs;

        let final_tx = tx.into();

        // Store tx into the block db transaction
        tx_st_transaction
            .as_mut()
            .storage::<ProcessedTransactions>()
            .insert(&tx_id, &())?;

        // Update `execution_data` data only after all steps.
        execution_data.coinbase = execution_data
            .coinbase
            .checked_add(tx_fee)
            .ok_or(ExecutorError::FeeOverflow)?;
        execution_data.used_gas = execution_data.used_gas.saturating_add(used_gas);
        execution_data
            .message_ids
            .extend(receipts.iter().filter_map(|r| r.message_id()));

        let status = if reverted {
            self.log_backtrace(&vm, &receipts);
            TransactionExecutionResult::Failed {
                result: Some(state),
                receipts,
            }
        } else {
            // else tx was a success
            TransactionExecutionResult::Success {
                result: Some(state),
                receipts,
            }
        };

        // queue up status for this tx to be stored once block id is finalized.
        execution_data.tx_status.push(TransactionExecutionStatus {
            id: tx_id,
            result: status,
        });

        Ok(final_tx)
    }

    fn verify_input_state(
        &self,
        db: &D,
        inputs: &[Input],
        block_height: BlockHeight,
        block_da_height: DaBlockHeight,
    ) -> ExecutorResult<()> {
        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    if let Some(coin) = db.storage::<Coins>().get(utxo_id)? {
                        let coin_mature_height = coin
                            .tx_pointer()
                            .block_height()
                            .saturating_add(**coin.maturity())
                            .into();
                        if block_height < coin_mature_height {
                            return Err(TransactionValidityError::CoinHasNotMatured(
                                *utxo_id,
                            )
                            .into())
                        }

                        if !coin
                            .matches_input(input)
                            .expect("The input is a coin above")
                        {
                            return Err(
                                TransactionValidityError::CoinMismatch(*utxo_id).into()
                            )
                        }
                    } else {
                        return Err(
                            TransactionValidityError::CoinDoesNotExist(*utxo_id).into()
                        )
                    }
                }
                Input::Contract(contract) => {
                    if !db
                        .storage::<ContractsInfo>()
                        .contains_key(&contract.contract_id)?
                    {
                        return Err(TransactionValidityError::ContractDoesNotExist(
                            contract.contract_id,
                        )
                        .into())
                    }
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // Eagerly return already spent if status is known.
                    if db.storage::<SpentMessages>().contains_key(nonce)? {
                        return Err(
                            TransactionValidityError::MessageAlreadySpent(*nonce).into()
                        )
                    }
                    if let Some(message) = db.storage::<Messages>().get(nonce)? {
                        if message.da_height() > block_da_height {
                            return Err(TransactionValidityError::MessageSpendTooEarly(
                                *nonce,
                            )
                            .into())
                        }

                        if !message
                            .matches_input(input)
                            .expect("The input is message above")
                        {
                            return Err(
                                TransactionValidityError::MessageMismatch(*nonce).into()
                            )
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

    /// Mark input utxos as spent
    fn spend_input_utxos(
        &self,
        inputs: &[Input],
        db: &mut D,
        reverted: bool,
        execution_data: &mut ExecutionData,
    ) -> ExecutorResult<()> {
        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    maturity,
                    ..
                })
                | Input::CoinPredicate(CoinPredicate {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    maturity,
                    ..
                }) => {
                    // prune utxo from db
                    let coin = db
                        .storage::<Coins>()
                        .remove(utxo_id)
                        .map_err(Into::into)
                        .transpose()
                        .unwrap_or_else(|| {
                            // If the coin is not found in the database, it means that it was
                            // already spent or `utxo_validation` is `false`.
                            self.get_coin_or_default(
                                db, *utxo_id, *owner, *amount, *asset_id, *maturity,
                            )
                        })?;

                    execution_data
                        .events
                        .push(ExecutorEvent::CoinConsumed(coin.uncompress(*utxo_id)));
                }
                Input::MessageDataSigned(_) | Input::MessageDataPredicate(_)
                    if reverted =>
                {
                    // Don't spend the retryable messages if transaction is reverted
                    continue
                }
                Input::MessageCoinSigned(MessageCoinSigned { nonce, .. })
                | Input::MessageCoinPredicate(MessageCoinPredicate { nonce, .. })
                | Input::MessageDataSigned(MessageDataSigned { nonce, .. })
                | Input::MessageDataPredicate(MessageDataPredicate { nonce, .. }) => {
                    // `MessageDataSigned` and `MessageDataPredicate` are spent only if tx is not reverted
                    // mark message id as spent
                    let was_already_spent =
                        db.storage::<SpentMessages>().insert(nonce, &())?;
                    // ensure message wasn't already marked as spent
                    if was_already_spent.is_some() {
                        return Err(ExecutorError::MessageAlreadySpent(*nonce))
                    }
                    // cleanup message contents
                    let message = db
                        .storage::<Messages>()
                        .remove(nonce)?
                        .ok_or_else(|| ExecutorError::MessageAlreadySpent(*nonce))?;
                    execution_data
                        .events
                        .push(ExecutorEvent::MessageConsumed(message));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn total_fee_paid<Tx: Chargeable>(
        &self,
        tx: &Tx,
        max_fee: Word,
        receipts: &[Receipt],
    ) -> ExecutorResult<(Word, Word)> {
        let mut used_gas = 0;
        for r in receipts {
            if let Receipt::ScriptResult { gas_used, .. } = r {
                used_gas = *gas_used;
                break
            }
        }

        let fee = tx
            .refund_fee(
                self.config.consensus_parameters.gas_costs(),
                self.config.consensus_parameters.fee_params(),
                used_gas,
            )
            .ok_or(ExecutorError::FeeOverflow)?;
        // if there's no script result (i.e. create) then fee == base amount
        Ok((
            used_gas,
            max_fee
                .checked_sub(fee)
                .expect("Refunded fee can't be more than `max_fee`."),
        ))
    }

    /// Computes all zeroed or variable inputs.
    /// In production mode, updates the inputs with computed values.
    /// In validation mode, compares the inputs with computed inputs.
    fn compute_inputs(
        &self,
        inputs: ExecutionTypes<&mut [Input], &[Input]>,
        tx_id: TxId,
        db: &mut D,
    ) -> ExecutorResult<()> {
        match inputs {
            ExecutionTypes::DryRun(inputs) | ExecutionTypes::Production(inputs) => {
                for input in inputs {
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
                                db, *utxo_id, *owner, *amount, *asset_id, *maturity,
                            )?;
                            *tx_pointer = *coin.tx_pointer();
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
                                contract.validated_utxo(self.options.utxo_validation)?;
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
            ExecutionTypes::Validation(inputs) => {
                for input in inputs {
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
                                db, *utxo_id, *owner, *amount, *asset_id, *maturity,
                            )?;
                            if tx_pointer != coin.tx_pointer() {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx_id,
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
                                != contract
                                    .validated_utxo(self.options.utxo_validation)?
                            {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx_id,
                                })
                            }
                            if balance_root != &contract.balance_root()? {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx_id,
                                })
                            }
                            if state_root != &contract.state_root()? {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx_id,
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

    #[allow(clippy::type_complexity)]
    // TODO: Maybe we need move it to `fuel-vm`? O_o Because other `Outputs` are processed there
    /// Computes all zeroed or variable outputs.
    /// In production mode, updates the outputs with computed values.
    /// In validation mode, compares the outputs with computed inputs.
    fn compute_not_utxo_outputs(
        &self,
        tx: ExecutionTypes<(&mut [Output], &[Input]), (&[Output], &[Input])>,
        tx_id: TxId,
        db: &mut D,
    ) -> ExecutorResult<()> {
        match tx {
            ExecutionTypes::DryRun(tx) | ExecutionTypes::Production(tx) => {
                for output in tx.0.iter_mut() {
                    if let Output::Contract(contract_output) = output {
                        let contract_id =
                            if let Some(Input::Contract(Contract {
                                contract_id, ..
                            })) = tx.1.get(contract_output.input_index as usize)
                            {
                                contract_id
                            } else {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx_id,
                                })
                            };

                        let mut contract = ContractRef::new(&mut *db, *contract_id);
                        contract_output.balance_root = contract.balance_root()?;
                        contract_output.state_root = contract.state_root()?;
                    }
                }
            }
            ExecutionTypes::Validation(tx) => {
                for output in tx.0 {
                    if let Output::Contract(contract_output) = output {
                        let contract_id =
                            if let Some(Input::Contract(Contract {
                                contract_id, ..
                            })) = tx.1.get(contract_output.input_index as usize)
                            {
                                contract_id
                            } else {
                                return Err(ExecutorError::InvalidTransactionOutcome {
                                    transaction_id: tx_id,
                                })
                            };

                        let mut contract = ContractRef::new(&mut *db, *contract_id);
                        if contract_output.balance_root != contract.balance_root()? {
                            return Err(ExecutorError::InvalidTransactionOutcome {
                                transaction_id: tx_id,
                            })
                        }
                        if contract_output.state_root != contract.state_root()? {
                            return Err(ExecutorError::InvalidTransactionOutcome {
                                transaction_id: tx_id,
                            })
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_coin_or_default(
        &self,
        db: &mut D,
        utxo_id: UtxoId,
        owner: Address,
        amount: u64,
        asset_id: AssetId,
        maturity: BlockHeight,
    ) -> ExecutorResult<CompressedCoin> {
        if self.options.utxo_validation {
            db.storage::<Coins>()
                .get(&utxo_id)?
                .ok_or(ExecutorError::TransactionValidity(
                    TransactionValidityError::CoinDoesNotExist(utxo_id),
                ))
                .map(Cow::into_owned)
        } else {
            // if utxo validation is disabled, just assign this new input to the original block
            let coin = CompressedCoinV1 {
                owner,
                amount,
                asset_id,
                maturity,
                tx_pointer: Default::default(),
            }
            .into();
            Ok(coin)
        }
    }

    /// Log a VM backtrace if configured to do so
    fn log_backtrace<Tx>(
        &self,
        vm: &Interpreter<VmStorage<D>, Tx>,
        receipts: &[Receipt],
    ) {
        if self.config.backtrace {
            if let Some(backtrace) = receipts
                .iter()
                .find_map(Receipt::result)
                .copied()
                .map(|result| FuelBacktrace::from_vm_error(vm, result))
            {
                let sp = usize::try_from(backtrace.registers()[RegId::SP]).expect(
                    "The `$sp` register points to the memory of the VM. \
                    Because the VM's memory is limited by the `usize` of the system, \
                    it is impossible to lose higher bits during truncation.",
                );
                warn!(
                    target = "vm",
                    "Backtrace on contract: 0x{:x}\nregisters: {:?}\ncall_stack: {:?}\nstack\n: {}",
                    backtrace.contract(),
                    backtrace.registers(),
                    backtrace.call_stack(),
                    hex::encode(&backtrace.memory()[..sp]), // print stack
                );
            }
        }
    }

    fn persist_output_utxos(
        &self,
        block_height: BlockHeight,
        execution_data: &mut ExecutionData,
        tx_id: &Bytes32,
        db: &mut D,
        inputs: &[Input],
        outputs: &[Output],
    ) -> ExecutorResult<()> {
        let tx_idx = execution_data.tx_count;
        for (output_index, output) in outputs.iter().enumerate() {
            let index = u8::try_from(output_index)
                .expect("Transaction can have only up to `u8::MAX` outputs");
            let utxo_id = UtxoId::new(*tx_id, index);
            match output {
                Output::Coin {
                    amount,
                    asset_id,
                    to,
                } => Self::insert_coin(
                    block_height,
                    execution_data,
                    utxo_id,
                    amount,
                    asset_id,
                    to,
                    db,
                )?,
                Output::Contract(contract) => {
                    if let Some(Input::Contract(Contract { contract_id, .. })) =
                        inputs.get(contract.input_index as usize)
                    {
                        db.storage::<ContractsLatestUtxo>().insert(
                            contract_id,
                            &ContractUtxoInfo {
                                utxo_id,
                                tx_pointer: TxPointer::new(block_height, tx_idx),
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
                    execution_data,
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
                    execution_data,
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
                            tx_pointer: TxPointer::new(block_height, tx_idx),
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    fn insert_coin(
        block_height: BlockHeight,
        execution_data: &mut ExecutionData,
        utxo_id: UtxoId,
        amount: &Word,
        asset_id: &AssetId,
        to: &Address,
        db: &mut D,
    ) -> ExecutorResult<()> {
        // Only insert a coin output if it has some amount.
        // This is because variable or transfer outputs won't have any value
        // if there's a revert or panic and shouldn't be added to the utxo set.
        if *amount > Word::MIN {
            let coin = CompressedCoinV1 {
                owner: *to,
                amount: *amount,
                asset_id: *asset_id,
                maturity: 0u32.into(),
                tx_pointer: TxPointer::new(block_height, execution_data.tx_count),
            }
            .into();

            if db.storage::<Coins>().insert(&utxo_id, &coin)?.is_some() {
                return Err(ExecutorError::OutputAlreadyExists)
            }
            execution_data
                .events
                .push(ExecutorEvent::CoinCreated(coin.uncompress(utxo_id)));
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
        self.fee.max_fee()
    }

    fn min_fee(&self) -> Word {
        self.fee.min_fee()
    }
}

impl Fee for CreateCheckedMetadata {
    fn max_fee(&self) -> Word {
        self.fee.max_fee()
    }

    fn min_fee(&self) -> Word {
        self.fee.min_fee()
    }
}
