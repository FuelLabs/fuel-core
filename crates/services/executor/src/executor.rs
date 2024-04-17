use crate::{
    ports::{
        MaybeCheckedTransaction,
        RelayerPort,
        TransactionsSource,
    },
    refs::ContractRef,
};
use block_component::*;
use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    structured_storage::StructuredStorage,
    tables::{
        Coins,
        ConsensusParametersVersions,
        ContractsLatestUtxo,
        FuelBlocks,
        Messages,
        ProcessedTransactions,
        SpentMessages,
    },
    transactional::{
        Changes,
        ConflictPolicy,
        IntoTransaction,
        Modifiable,
        ReadTransaction,
        StorageTransaction,
        WriteTransaction,
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
        RelayedTransaction,
    },
    fuel_asm::{
        RegId,
        Word,
    },
    fuel_merkle::binary::root_calculator::MerkleRootCalculator,
    fuel_tx::{
        field::{
            InputContract,
            MaxFeeLimit,
            MintAmount,
            MintAssetId,
            MintGasPrice,
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
        ConsensusParameters,
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
        canonical::Deserialize,
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
            UpgradeCheckedMetadata,
            UploadCheckedMetadata,
        },
        interpreter::{
            CheckedMetadata as CheckedMetadataTrait,
            ExecutableTransaction,
            InterpreterParams,
        },
        state::StateTransition,
        Backtrace as FuelBacktrace,
        Interpreter,
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
            ForcedTransactionFailure,
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
use std::borrow::Cow;
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
    changes: Changes,
    pub skipped_transactions: Vec<(TxId, ExecutorError)>,
    event_inbox_root: Bytes32,
}

/// Per-block execution options
#[derive(serde::Serialize, serde::Deserialize, Clone, Default, Debug)]
pub struct ExecutionOptions {
    /// UTXO Validation flag, when disabled the executor skips signature and UTXO existence checks
    pub utxo_validation: bool,
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,
}

/// The executor instance performs block production and validation. Given a block, it will execute all
/// the transactions contained in the block and persist changes to the underlying database as needed.
/// In production mode, block fields like transaction commitments are set based on the executed txs.
/// In validation mode, the processed block commitments are compared with the proposed block.
#[derive(Clone, Debug)]
pub struct ExecutionInstance<R, D> {
    pub relayer: R,
    pub database: D,
    pub options: ExecutionOptions,
}

impl<R, D> ExecutionInstance<R, D>
where
    R: RelayerPort,
    D: KeyValueInspect<Column = Column>,
{
    pub fn execute_without_commit<TxSource>(
        self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<UncommittedResult<Changes>>
    where
        TxSource: TransactionsSource,
    {
        self.execute_inner(block)
    }
}

// TODO: Make this module private after moving unit tests from `fuel-core` here.
pub mod block_component {
    use super::*;

    pub struct PartialBlockComponent<'a, TxSource> {
        pub empty_block: &'a mut PartialFuelBlock,
        pub transactions_source: TxSource,
        pub coinbase_contract_id: ContractId,
        pub gas_price: u64,
        /// The private marker to allow creation of the type only by constructor.
        _marker: core::marker::PhantomData<()>,
    }

    impl<'a> PartialBlockComponent<'a, OnceTransactionsSource> {
        pub fn from_partial_block(block: &'a mut PartialFuelBlock) -> Self {
            let transaction = core::mem::take(&mut block.transactions);
            let (gas_price, coinbase_contract_id) =
                if let Some(Transaction::Mint(mint)) = transaction.last() {
                    (*mint.gas_price(), mint.input_contract().contract_id)
                } else {
                    (0, Default::default())
                };

            Self {
                empty_block: block,
                transactions_source: OnceTransactionsSource::new(transaction),
                coinbase_contract_id,
                gas_price,
                _marker: Default::default(),
            }
        }
    }

    impl<'a, TxSource> PartialBlockComponent<'a, TxSource> {
        pub fn from_component(
            block: &'a mut PartialFuelBlock,
            transactions_source: TxSource,
            coinbase_contract_id: ContractId,
            gas_price: u64,
        ) -> Self {
            debug_assert!(block.transactions.is_empty());
            PartialBlockComponent {
                empty_block: block,
                transactions_source,
                gas_price,
                coinbase_contract_id,
                _marker: Default::default(),
            }
        }
    }
}

impl<R, D> ExecutionInstance<R, D>
where
    R: RelayerPort,
    D: KeyValueInspect<Column = Column>,
{
    #[tracing::instrument(skip_all)]
    fn execute_inner<TxSource>(
        self,
        block: ExecutionBlockWithSource<TxSource>,
    ) -> ExecutorResult<UncommittedResult<Changes>>
    where
        TxSource: TransactionsSource,
    {
        // Compute the block id before execution if there is one.
        let pre_exec_block_id = block.id();

        // If there is full fuel block for validation then map it into
        // a partial header.
        let block = block.map_v(PartialFuelBlock::from);

        let (block, execution_data) = match block {
            ExecutionTypes::DryRun(component) => {
                let mut block =
                    PartialFuelBlock::new(component.header_to_produce, vec![]);
                let block_executor = BlockExecutor::new(
                    self.relayer,
                    self.database,
                    self.options,
                    &block,
                )?;
                let component = PartialBlockComponent::from_component(
                    &mut block,
                    component.transactions_source,
                    component.coinbase_recipient,
                    component.gas_price,
                );
                let execution_data =
                    block_executor.execute_block(ExecutionType::DryRun(component))?;

                (block, execution_data)
            }
            ExecutionTypes::Production(component) => {
                let mut block =
                    PartialFuelBlock::new(component.header_to_produce, vec![]);
                let block_executor = BlockExecutor::new(
                    self.relayer,
                    self.database,
                    self.options,
                    &block,
                )?;

                let component = PartialBlockComponent::from_component(
                    &mut block,
                    component.transactions_source,
                    component.coinbase_recipient,
                    component.gas_price,
                );

                let execution_data =
                    block_executor.execute_block(ExecutionType::Production(component))?;

                (block, execution_data)
            }
            ExecutionTypes::Validation(mut block) => {
                let block_executor = BlockExecutor::new(
                    self.relayer,
                    self.database,
                    self.options,
                    &block,
                )?;

                let component = PartialBlockComponent::from_partial_block(&mut block);
                let execution_data =
                    block_executor.execute_block(ExecutionType::Validation(component))?;
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
            changes,
            event_inbox_root,
            ..
        } = execution_data;

        // Now that the transactions have been executed, generate the full header.

        let block = block.generate(&message_ids[..], event_inbox_root);

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
        Ok(UncommittedResult::new(result, changes))
    }
}

#[derive(Clone, Debug)]
pub struct BlockExecutor<R, D> {
    relayer: R,
    block_st_transaction: StorageTransaction<D>,
    consensus_params: ConsensusParameters,
    options: ExecutionOptions,
}

impl<R, D> BlockExecutor<R, D>
where
    D: KeyValueInspect<Column = Column>,
{
    pub fn new(
        relayer: R,
        database: D,
        options: ExecutionOptions,
        block: &PartialFuelBlock,
    ) -> ExecutorResult<Self> {
        let consensus_params_version = block.header.consensus_parameters_version;

        let block_st_transaction = database
            .into_transaction()
            .with_policy(ConflictPolicy::Overwrite);
        let consensus_params = block_st_transaction
            .storage::<ConsensusParametersVersions>()
            .get(&consensus_params_version)?
            .ok_or(ExecutorError::ConsensusParametersNotFound(
                consensus_params_version,
            ))?
            .into_owned();

        Ok(Self {
            relayer,
            block_st_transaction,
            consensus_params,
            options,
        })
    }
}

impl<R, D> BlockExecutor<R, D>
where
    R: RelayerPort,
    D: KeyValueInspect<Column = Column>,
{
    #[tracing::instrument(skip_all)]
    /// Execute the fuel block with all transactions.
    fn execute_block<TxSource>(
        mut self,
        block: ExecutionType<PartialBlockComponent<TxSource>>,
    ) -> ExecutorResult<ExecutionData>
    where
        TxSource: TransactionsSource,
    {
        let block_gas_limit = self.consensus_params.block_gas_limit();
        let mut data = ExecutionData {
            coinbase: 0,
            used_gas: 0,
            tx_count: 0,
            found_mint: false,
            message_ids: Vec::new(),
            tx_status: Vec::new(),
            events: Vec::new(),
            changes: Default::default(),
            skipped_transactions: Vec::new(),
            event_inbox_root: Default::default(),
        };
        let execution_data = &mut data;

        // Split out the execution kind and partial block.
        let (execution_kind, component) = block.split();
        let block = component.empty_block;
        let source = component.transactions_source;
        let gas_price = component.gas_price;
        let coinbase_contract_id = component.coinbase_contract_id;
        let block_height = *block.header.height();

        let forced_transactions = if self.relayer.enabled() {
            self.process_da(&block.header, execution_data)?
        } else {
            Vec::with_capacity(0)
        };

        // The block level storage transaction that also contains data from the relayer.
        // Starting from this point, modifications from each thread should be independent
        // and shouldn't touch the same data.
        let mut block_with_relayer_data_transaction = self.block_st_transaction.read_transaction()
            // Enforces independent changes from each thread.
            .with_policy(ConflictPolicy::Fail);

        // We execute transactions in a single thread right now, but later,
        // we will execute them in parallel with a separate independent storage transaction per thread.
        let mut thread_block_transaction = block_with_relayer_data_transaction
            .read_transaction()
            .with_policy(ConflictPolicy::Overwrite);

        debug_assert!(block.transactions.is_empty());

        let mut execute_transaction = |execution_data: &mut ExecutionData,
                                       tx: MaybeCheckedTransaction,
                                       gas_price: Word|
         -> ExecutorResult<()> {
            let tx_count = execution_data.tx_count;
            let tx = {
                let mut tx_st_transaction = thread_block_transaction
                    .write_transaction()
                    .with_policy(ConflictPolicy::Overwrite);
                let tx_id = tx.id(&self.consensus_params.chain_id());
                let tx = self.execute_transaction(
                    tx,
                    &tx_id,
                    &block.header,
                    coinbase_contract_id,
                    gas_price,
                    execution_data,
                    execution_kind,
                    &mut tx_st_transaction,
                )?;
                tx_st_transaction.commit()?;
                tx
            };

            block.transactions.push(tx);
            execution_data.tx_count = tx_count
                .checked_add(1)
                .ok_or(ExecutorError::TooManyTransactions)?;

            Ok(())
        };

        let relayed_tx_iter = forced_transactions.into_iter();
        for transaction in relayed_tx_iter {
            const RELAYED_GAS_PRICE: Word = 0;
            let transaction = MaybeCheckedTransaction::CheckedTransaction(transaction);
            let tx_id = transaction.id(&self.consensus_params.chain_id());
            match execute_transaction(
                &mut *execution_data,
                transaction,
                RELAYED_GAS_PRICE,
            ) {
                Ok(_) => {}
                Err(err) => {
                    let event = ExecutorEvent::ForcedTransactionFailed {
                        id: tx_id.into(),
                        block_height,
                        failure: err.to_string(),
                    };
                    execution_data.events.push(event);
                }
            }
        }

        let remaining_gas_limit = block_gas_limit.saturating_sub(execution_data.used_gas);

        // L2 originated transactions should be in the `TxSource`. This will be triggered after
        // all relayed transactions are processed.
        let mut regular_tx_iter = source.next(remaining_gas_limit).into_iter().peekable();
        while regular_tx_iter.peek().is_some() {
            for transaction in regular_tx_iter {
                let tx_id = transaction.id(&self.consensus_params.chain_id());
                match execute_transaction(&mut *execution_data, transaction, gas_price) {
                    Ok(_) => {}
                    Err(err) => match execution_kind {
                        ExecutionKind::Production => {
                            execution_data.skipped_transactions.push((tx_id, err));
                        }
                        ExecutionKind::DryRun | ExecutionKind::Validation => {
                            return Err(err);
                        }
                    },
                }
            }

            let new_remaining_gas_limit =
                block_gas_limit.saturating_sub(execution_data.used_gas);

            regular_tx_iter = source.next(new_remaining_gas_limit).into_iter().peekable();
        }

        // After the execution of all transactions in production mode, we can set the final fee.
        if execution_kind == ExecutionKind::Production {
            let amount_to_mint = if coinbase_contract_id != ContractId::zeroed() {
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
                    contract_id: coinbase_contract_id,
                },
                output::contract::Contract {
                    input_index: 0,
                    balance_root: Bytes32::zeroed(),
                    state_root: Bytes32::zeroed(),
                },
                amount_to_mint,
                *self.consensus_params.base_asset_id(),
                gas_price,
            );

            execute_transaction(
                execution_data,
                MaybeCheckedTransaction::Transaction(coinbase_tx.into()),
                gas_price,
            )?;
        }

        let changes_from_thread = thread_block_transaction.into_changes();
        block_with_relayer_data_transaction.commit_changes(changes_from_thread)?;
        self.block_st_transaction
            .commit_changes(block_with_relayer_data_transaction.into_changes())?;

        if execution_kind != ExecutionKind::DryRun && !data.found_mint {
            return Err(ExecutorError::MintMissing)
        }

        data.changes = self.block_st_transaction.into_changes();

        Ok(data)
    }

    fn process_da(
        &mut self,
        header: &PartialBlockHeader,
        execution_data: &mut ExecutionData,
    ) -> ExecutorResult<Vec<CheckedTransaction>> {
        let block_height = *header.height();
        let prev_block_height = block_height
            .pred()
            .ok_or(ExecutorError::ExecutingGenesisBlock)?;

        let prev_block_header = self
            .block_st_transaction
            .storage::<FuelBlocks>()
            .get(&prev_block_height)?
            .ok_or(ExecutorError::PreviousBlockIsNotFound)?;
        let previous_da_height = prev_block_header.header().da_height;
        let Some(next_unprocessed_da_height) = previous_da_height.0.checked_add(1) else {
            return Err(ExecutorError::DaHeightExceededItsLimit)
        };

        let mut root_calculator = MerkleRootCalculator::new();

        let mut checked_forced_txs = vec![];

        for da_height in next_unprocessed_da_height..=header.da_height.0 {
            let da_height = da_height.into();
            let events = self
                .relayer
                .get_events(&da_height)
                .map_err(|err| ExecutorError::RelayerError(err.to_string()))?;
            for event in events {
                root_calculator.push(event.hash().as_ref());
                match event {
                    Event::Message(message) => {
                        if message.da_height() != da_height {
                            return Err(ExecutorError::RelayerGivesIncorrectMessages)
                        }
                        self.block_st_transaction
                            .storage_as_mut::<Messages>()
                            .insert(message.nonce(), &message)?;
                        execution_data
                            .events
                            .push(ExecutorEvent::MessageImported(message));
                    }
                    Event::Transaction(relayed_tx) => {
                        let id = relayed_tx.id();
                        // perform basic checks
                        let checked_tx_res = Self::validate_forced_tx(
                            relayed_tx,
                            header,
                            &self.consensus_params,
                        );
                        // handle the result
                        match checked_tx_res {
                            Ok(checked_tx) => {
                                checked_forced_txs.push(checked_tx);
                            }
                            Err(err) => {
                                execution_data.events.push(
                                    ExecutorEvent::ForcedTransactionFailed {
                                        id,
                                        block_height,
                                        failure: err.to_string(),
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }

        execution_data.event_inbox_root = root_calculator.root().into();

        Ok(checked_forced_txs)
    }

    /// Parse forced transaction payloads and perform basic checks
    fn validate_forced_tx(
        relayed_tx: RelayedTransaction,
        header: &PartialBlockHeader,
        consensus_params: &ConsensusParameters,
    ) -> Result<CheckedTransaction, ForcedTransactionFailure> {
        let parsed_tx = Self::parse_tx_bytes(&relayed_tx)?;
        Self::tx_is_valid_variant(&parsed_tx)?;
        Self::relayed_tx_claimed_enough_max_gas(
            &parsed_tx,
            &relayed_tx,
            consensus_params,
        )?;
        let checked_tx =
            Self::get_checked_tx(parsed_tx, *header.height(), consensus_params)?;
        Ok(CheckedTransaction::from(checked_tx))
    }

    fn parse_tx_bytes(
        relayed_transaction: &RelayedTransaction,
    ) -> Result<Transaction, ForcedTransactionFailure> {
        let tx_bytes = relayed_transaction.serialized_transaction();
        let tx = Transaction::from_bytes(tx_bytes)
            .map_err(|_| ForcedTransactionFailure::CodecError)?;
        Ok(tx)
    }

    fn get_checked_tx(
        tx: Transaction,
        height: BlockHeight,
        consensus_params: &ConsensusParameters,
    ) -> Result<Checked<Transaction>, ForcedTransactionFailure> {
        let checked_tx = tx
            .into_checked(height, consensus_params)
            .map_err(ForcedTransactionFailure::CheckError)?;
        Ok(checked_tx)
    }

    fn tx_is_valid_variant(tx: &Transaction) -> Result<(), ForcedTransactionFailure> {
        match tx {
            Transaction::Mint(_) => Err(ForcedTransactionFailure::InvalidTransactionType),
            Transaction::Script(_)
            | Transaction::Create(_)
            | Transaction::Upgrade(_)
            | Transaction::Upload(_) => Ok(()),
        }
    }

    fn relayed_tx_claimed_enough_max_gas(
        tx: &Transaction,
        relayed_tx: &RelayedTransaction,
        consensus_params: &ConsensusParameters,
    ) -> Result<(), ForcedTransactionFailure> {
        let claimed_max_gas = relayed_tx.max_gas();
        let gas_costs = consensus_params.gas_costs();
        let fee_params = consensus_params.fee_params();
        let actual_max_gas = match tx {
            Transaction::Script(tx) => tx.max_gas(gas_costs, fee_params),
            Transaction::Create(tx) => tx.max_gas(gas_costs, fee_params),
            Transaction::Mint(_) => {
                return Err(ForcedTransactionFailure::InvalidTransactionType)
            }
            Transaction::Upgrade(tx) => tx.max_gas(gas_costs, fee_params),
            Transaction::Upload(tx) => tx.max_gas(gas_costs, fee_params),
        };
        if actual_max_gas > claimed_max_gas {
            return Err(ForcedTransactionFailure::InsufficientMaxGas {
                claimed_max_gas,
                actual_max_gas,
            });
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_transaction<T>(
        &self,
        tx: MaybeCheckedTransaction,
        tx_id: &TxId,
        header: &PartialBlockHeader,
        coinbase_contract_id: ContractId,
        gas_price: Word,
        execution_data: &mut ExecutionData,
        execution_kind: ExecutionKind,
        tx_st_transaction: &mut StorageTransaction<T>,
    ) -> ExecutorResult<Transaction>
    where
        T: KeyValueInspect<Column = Column>,
    {
        if execution_data.found_mint {
            return Err(ExecutorError::MintIsNotLastTransaction)
        }

        // Throw a clear error if the transaction id is a duplicate
        if tx_st_transaction
            .storage::<ProcessedTransactions>()
            .contains_key(tx_id)?
        {
            return Err(ExecutorError::TransactionIdCollision(*tx_id))
        }

        let block_height = *header.height();
        let checked_tx = match tx {
            MaybeCheckedTransaction::Transaction(tx) => tx
                .into_checked_basic(block_height, &self.consensus_params)?
                .into(),
            MaybeCheckedTransaction::CheckedTransaction(checked_tx) => checked_tx,
        };

        match checked_tx {
            CheckedTransaction::Script(tx) => self.execute_chargeable_transaction(
                tx,
                header,
                coinbase_contract_id,
                gas_price,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
            CheckedTransaction::Create(tx) => self.execute_chargeable_transaction(
                tx,
                header,
                coinbase_contract_id,
                gas_price,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
            CheckedTransaction::Mint(mint) => self.execute_mint(
                mint,
                header,
                coinbase_contract_id,
                gas_price,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
            CheckedTransaction::Upgrade(tx) => self.execute_chargeable_transaction(
                tx,
                header,
                coinbase_contract_id,
                gas_price,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
            CheckedTransaction::Upload(tx) => self.execute_chargeable_transaction(
                tx,
                header,
                coinbase_contract_id,
                gas_price,
                execution_data,
                tx_st_transaction,
                execution_kind,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_mint<T>(
        &self,
        checked_mint: Checked<Mint>,
        header: &PartialBlockHeader,
        coinbase_contract_id: ContractId,
        gas_price: Word,
        execution_data: &mut ExecutionData,
        block_st_transaction: &mut StorageTransaction<T>,
        execution_kind: ExecutionKind,
    ) -> ExecutorResult<Transaction>
    where
        T: KeyValueInspect<Column = Column>,
    {
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
            if *mint.gas_price() != gas_price {
                return Err(ExecutorError::CoinbaseGasPriceMismatch)
            }

            let block_height = *header.height();

            let input = mint.input_contract().clone();
            let output = *mint.output_contract();
            let mut inputs = [Input::Contract(input)];
            let mut outputs = [Output::Contract(output)];

            if self.options.utxo_validation {
                // validate utxos exist
                self.verify_input_state(
                    block_st_transaction,
                    inputs.as_mut_slice(),
                    header.da_height,
                )?;
            }

            match execution_kind {
                ExecutionKind::DryRun | ExecutionKind::Production => {
                    self.compute_inputs(inputs.as_mut_slice(), block_st_transaction)?;
                }
                ExecutionKind::Validation => {
                    self.validate_inputs_state(
                        inputs.as_mut_slice(),
                        coinbase_id,
                        block_st_transaction,
                    )?;
                }
            }

            let mut sub_block_db_commit = block_st_transaction
                .write_transaction()
                .with_policy(ConflictPolicy::Overwrite);

            let mut vm_db = VmStorage::new(
                &mut sub_block_db_commit,
                &header.consensus,
                &header.application,
                coinbase_contract_id,
            );

            fuel_vm::interpreter::contract::balance_increase(
                &mut vm_db,
                &mint.input_contract().contract_id,
                mint.mint_asset_id(),
                *mint.mint_amount(),
            )
            .map_err(|e| format!("{e}"))
            .map_err(ExecutorError::CoinbaseCannotIncreaseBalance)?;
            sub_block_db_commit.commit()?;

            self.persist_output_utxos(
                block_height,
                execution_data,
                &coinbase_id,
                block_st_transaction,
                inputs.as_slice(),
                outputs.as_slice(),
            )?;
            self.compute_state_of_not_utxo_outputs(
                outputs.as_mut_slice(),
                inputs.as_slice(),
                coinbase_id,
                block_st_transaction,
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
                total_gas: 0,
                total_fee: 0,
            },
        });

        if block_st_transaction
            .storage::<ProcessedTransactions>()
            .insert(&coinbase_id, &())?
            .is_some()
        {
            return Err(ExecutorError::TransactionIdCollision(coinbase_id))
        }
        Ok(tx)
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_chargeable_transaction<Tx, T>(
        &self,
        mut checked_tx: Checked<Tx>,
        header: &PartialBlockHeader,
        coinbase_contract_id: ContractId,
        gas_price: Word,
        execution_data: &mut ExecutionData,
        tx_st_transaction: &mut StorageTransaction<T>,
        execution_kind: ExecutionKind,
    ) -> ExecutorResult<Transaction>
    where
        Tx: ExecutableTransaction + PartialEq + Cacheable + Send + Sync + 'static,
        <Tx as IntoChecked>::Metadata: CheckedMetadata,
        T: KeyValueInspect<Column = Column>,
    {
        let tx_id = checked_tx.id();
        let min_gas = checked_tx.metadata().min_gas();
        let max_fee = checked_tx.transaction().max_fee_limit();

        if self.options.utxo_validation {
            checked_tx = checked_tx
                .check_predicates(&CheckPredicateParams::from(&self.consensus_params))
                .map_err(|e| {
                    ExecutorError::TransactionValidity(
                        TransactionValidityError::Validation(e),
                    )
                })?;
            debug_assert!(checked_tx.checks().contains(Checks::Predicates));

            // validate utxos exist and maturity is properly set
            self.verify_input_state(
                tx_st_transaction,
                checked_tx.transaction().inputs(),
                header.da_height,
            )?;
            // validate transaction signature
            checked_tx = checked_tx
                .check_signatures(&self.consensus_params.chain_id())
                .map_err(TransactionValidityError::from)?;
            debug_assert!(checked_tx.checks().contains(Checks::Signatures));
        }

        if execution_kind == ExecutionKind::Validation {
            self.validate_inputs_state(
                checked_tx.transaction().inputs(),
                tx_id,
                tx_st_transaction,
            )?;
        }

        // execute transaction
        // setup database view that only lives for the duration of vm execution
        let mut sub_block_db_commit = tx_st_transaction
            .read_transaction()
            .with_policy(ConflictPolicy::Overwrite);

        // execution vm
        let vm_db = VmStorage::new(
            &mut sub_block_db_commit,
            &header.consensus,
            &header.application,
            coinbase_contract_id,
        );

        let mut vm = Interpreter::with_storage(
            vm_db,
            InterpreterParams::new(gas_price, &self.consensus_params),
        );

        let gas_costs = self.consensus_params.gas_costs();
        let fee_params = self.consensus_params.fee_params();

        let ready_tx = checked_tx
            .clone()
            .into_ready(gas_price, gas_costs, fee_params)?;

        let vm_result: StateTransition<_> = vm
            .transact(ready_tx)
            .map_err(|error| ExecutorError::VmExecution {
                error: error.to_string(),
                transaction_id: tx_id,
            })?
            .into();
        let reverted = vm_result.should_revert();

        let (state, mut tx, receipts): (_, Tx, _) = vm_result.into_inner();
        #[cfg(debug_assertions)]
        {
            tx.precompute(&self.consensus_params.chain_id())?;
            debug_assert_eq!(tx.id(&self.consensus_params.chain_id()), tx_id);
        }

        for (original_input, produced_input) in checked_tx
            .transaction()
            .inputs()
            .iter()
            .zip(tx.inputs_mut())
        {
            let predicate_gas_used = original_input.predicate_gas_used();

            if let Some(gas_used) = predicate_gas_used {
                match produced_input {
                    Input::CoinPredicate(CoinPredicate {
                        predicate_gas_used, ..
                    })
                    | Input::MessageCoinPredicate(MessageCoinPredicate {
                        predicate_gas_used,
                        ..
                    })
                    | Input::MessageDataPredicate(MessageDataPredicate {
                        predicate_gas_used,
                        ..
                    }) => {
                        *predicate_gas_used = gas_used;
                    }
                    _ => {
                        debug_assert!(false, "This error is not possible unless VM changes the order of inputs, \
                        or we added a new predicate inputs.");
                        return Err(ExecutorError::InvalidTransactionOutcome {
                            transaction_id: tx_id,
                        })
                    }
                }
            }
        }

        // We always need to update inputs with storage state before execution,
        // because VM zeroes malleable fields during the execution.
        self.compute_inputs(tx.inputs_mut(), tx_st_transaction)?;

        // only commit state changes if execution was a success
        if !reverted {
            self.log_backtrace(&vm, &receipts);
            let changes = sub_block_db_commit.into_changes();
            tx_st_transaction.commit_changes(changes)?;
        }

        // update block commitment
        let (used_gas, tx_fee) =
            self.total_fee_paid(&tx, min_gas, max_fee, &receipts, gas_price)?;

        // change the spent status of the tx inputs
        self.spend_input_utxos(tx.inputs(), tx_st_transaction, reverted, execution_data)?;

        // Persist utxos first and after calculate the not utxo outputs
        self.persist_output_utxos(
            *header.height(),
            execution_data,
            &tx_id,
            tx_st_transaction,
            tx.inputs(),
            tx.outputs(),
        )?;

        // We always need to update outputs with storage state after execution.
        let mut outputs = core::mem::take(tx.outputs_mut());
        self.compute_state_of_not_utxo_outputs(
            &mut outputs,
            tx.inputs(),
            tx_id,
            tx_st_transaction,
        )?;
        *tx.outputs_mut() = outputs;

        // The validator ensures that the generated transaction by him is the same as provided by the block producer.
        if execution_kind == ExecutionKind::Validation && &tx != checked_tx.transaction()
        {
            return Err(ExecutorError::InvalidTransactionOutcome {
                transaction_id: tx_id,
            })
        }

        let final_tx = tx.into();

        // Store tx into the block db transaction
        tx_st_transaction
            .storage::<ProcessedTransactions>()
            .insert(&tx_id, &())?;

        // Update `execution_data` data only after all steps.
        execution_data.coinbase = execution_data
            .coinbase
            .checked_add(tx_fee)
            .ok_or(ExecutorError::FeeOverflow)?;
        execution_data.used_gas = execution_data
            .used_gas
            .checked_add(used_gas)
            .ok_or(ExecutorError::GasOverflow)?;
        execution_data
            .message_ids
            .extend(receipts.iter().filter_map(|r| r.message_id()));

        let status = if reverted {
            TransactionExecutionResult::Failed {
                result: Some(state),
                receipts,
                total_gas: used_gas,
                total_fee: tx_fee,
            }
        } else {
            // else tx was a success
            TransactionExecutionResult::Success {
                result: Some(state),
                receipts,
                total_gas: used_gas,
                total_fee: tx_fee,
            }
        };

        // queue up status for this tx to be stored once block id is finalized.
        execution_data.tx_status.push(TransactionExecutionStatus {
            id: tx_id,
            result: status,
        });

        Ok(final_tx)
    }

    fn verify_input_state<T>(
        &self,
        db: &StorageTransaction<T>,
        inputs: &[Input],
        block_da_height: DaBlockHeight,
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned { utxo_id, .. })
                | Input::CoinPredicate(CoinPredicate { utxo_id, .. }) => {
                    if let Some(coin) = db.storage::<Coins>().get(utxo_id)? {
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
                        .storage::<ContractsLatestUtxo>()
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
                        );
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
    fn spend_input_utxos<T>(
        &self,
        inputs: &[Input],
        db: &mut StorageTransaction<T>,
        reverted: bool,
        execution_data: &mut ExecutionData,
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    ..
                })
                | Input::CoinPredicate(CoinPredicate {
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
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
                                db, *utxo_id, *owner, *amount, *asset_id,
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
                        .ok_or(ExecutorError::MessageAlreadySpent(*nonce))?;
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
        min_gas: Word,
        max_fee: Word,
        receipts: &[Receipt],
        gas_price: Word,
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
                self.consensus_params.gas_costs(),
                self.consensus_params.fee_params(),
                used_gas,
                gas_price,
            )
            .ok_or(ExecutorError::FeeOverflow)?;
        let total_used_gas = min_gas
            .checked_add(used_gas)
            .ok_or(ExecutorError::GasOverflow)?;
        // if there's no script result (i.e. create) then fee == base amount
        Ok((
            total_used_gas,
            max_fee
                .checked_sub(fee)
                .expect("Refunded fee can't be more than `max_fee`."),
        ))
    }

    /// Computes all zeroed or variable inputs.
    /// In production mode, updates the inputs with computed values.
    /// In validation mode, compares the inputs with computed inputs.
    fn compute_inputs<T>(
        &self,
        inputs: &mut [Input],
        db: &StorageTransaction<T>,
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned {
                    tx_pointer,
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    ..
                })
                | Input::CoinPredicate(CoinPredicate {
                    tx_pointer,
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    ..
                }) => {
                    let coin = self
                        .get_coin_or_default(db, *utxo_id, *owner, *amount, *asset_id)?;
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
                    let contract =
                        ContractRef::new(StructuredStorage::new(db), *contract_id);
                    let utxo_info =
                        contract.validated_utxo(self.options.utxo_validation)?;
                    *utxo_id = *utxo_info.utxo_id();
                    *tx_pointer = utxo_info.tx_pointer();
                    *balance_root = contract.balance_root()?;
                    *state_root = contract.state_root()?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_inputs_state<T>(
        &self,
        inputs: &[Input],
        tx_id: TxId,
        db: &StorageTransaction<T>,
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        for input in inputs {
            match input {
                Input::CoinSigned(CoinSigned {
                    tx_pointer,
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    ..
                })
                | Input::CoinPredicate(CoinPredicate {
                    tx_pointer,
                    utxo_id,
                    owner,
                    amount,
                    asset_id,
                    ..
                }) => {
                    let coin = self
                        .get_coin_or_default(db, *utxo_id, *owner, *amount, *asset_id)?;
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
                    let contract =
                        ContractRef::new(StructuredStorage::new(db), *contract_id);
                    let provided_info =
                        ContractUtxoInfo::V1((*utxo_id, *tx_pointer).into());
                    if provided_info
                        != contract.validated_utxo(self.options.utxo_validation)?
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
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    // TODO: Maybe we need move it to `fuel-vm`? O_o Because other `Outputs` are processed there
    /// Computes all zeroed or variable outputs.
    /// In production mode, updates the outputs with computed values.
    /// In validation mode, compares the outputs with computed inputs.
    fn compute_state_of_not_utxo_outputs<T>(
        &self,
        outputs: &mut [Output],
        inputs: &[Input],
        tx_id: TxId,
        db: &StorageTransaction<T>,
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        for output in outputs {
            if let Output::Contract(contract_output) = output {
                let contract_id =
                    if let Some(Input::Contract(Contract { contract_id, .. })) =
                        inputs.get(contract_output.input_index as usize)
                    {
                        contract_id
                    } else {
                        return Err(ExecutorError::InvalidTransactionOutcome {
                            transaction_id: tx_id,
                        })
                    };

                let contract = ContractRef::new(StructuredStorage::new(db), *contract_id);
                contract_output.balance_root = contract.balance_root()?;
                contract_output.state_root = contract.state_root()?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_coin_or_default<T>(
        &self,
        db: &StorageTransaction<T>,
        utxo_id: UtxoId,
        owner: Address,
        amount: u64,
        asset_id: AssetId,
    ) -> ExecutorResult<CompressedCoin>
    where
        T: KeyValueInspect<Column = Column>,
    {
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
                tx_pointer: Default::default(),
            }
            .into();
            Ok(coin)
        }
    }

    /// Log a VM backtrace if configured to do so
    fn log_backtrace<T, Tx>(
        &self,
        vm: &Interpreter<VmStorage<T>, Tx>,
        receipts: &[Receipt],
    ) {
        if self.options.backtrace {
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
                    hex::encode(backtrace.memory().read(0usize, sp).expect("`SP` always within stack")), // print stack
                );
            }
        }
    }

    fn persist_output_utxos<T>(
        &self,
        block_height: BlockHeight,
        execution_data: &mut ExecutionData,
        tx_id: &Bytes32,
        db: &mut StorageTransaction<T>,
        inputs: &[Input],
        outputs: &[Output],
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        let tx_idx = execution_data.tx_count;
        for (output_index, output) in outputs.iter().enumerate() {
            let index = u16::try_from(output_index)
                .expect("Transaction can have only up to `u16::MAX` outputs");
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
                        let tx_pointer = TxPointer::new(block_height, tx_idx);
                        db.storage::<ContractsLatestUtxo>().insert(
                            contract_id,
                            &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
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
                    let tx_pointer = TxPointer::new(block_height, tx_idx);
                    db.storage::<ContractsLatestUtxo>().insert(
                        contract_id,
                        &ContractUtxoInfo::V1((utxo_id, tx_pointer).into()),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn insert_coin<T>(
        block_height: BlockHeight,
        execution_data: &mut ExecutionData,
        utxo_id: UtxoId,
        amount: &Word,
        asset_id: &AssetId,
        to: &Address,
        db: &mut StorageTransaction<T>,
    ) -> ExecutorResult<()>
    where
        T: KeyValueInspect<Column = Column>,
    {
        // Only insert a coin output if it has some amount.
        // This is because variable or transfer outputs won't have any value
        // if there's a revert or panic and shouldn't be added to the utxo set.
        if *amount > Word::MIN {
            let coin = CompressedCoinV1 {
                owner: *to,
                amount: *amount,
                asset_id: *asset_id,
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

trait CheckedMetadata: CheckedMetadataTrait + Clone + Send + Sync {
    fn min_gas(&self) -> u64;
}

impl CheckedMetadata for ScriptCheckedMetadata {
    fn min_gas(&self) -> u64 {
        self.min_gas
    }
}

impl CheckedMetadata for CreateCheckedMetadata {
    fn min_gas(&self) -> u64 {
        self.min_gas
    }
}

impl CheckedMetadata for UpgradeCheckedMetadata {
    fn min_gas(&self) -> u64 {
        self.min_gas
    }
}

impl CheckedMetadata for UploadCheckedMetadata {
    fn min_gas(&self) -> u64 {
        self.min_gas
    }
}
