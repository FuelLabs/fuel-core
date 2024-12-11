use crate::{
    config::Config,
    dependency_splitter::DependencySplitter,
    one_core_tx_source::OneCoreTxSource,
};
use fuel_core_storage::{
    column::Column,
    iter::{
        changes_iterator::ChangesIterator,
        IteratorOverTable,
    },
    kv_store::KeyValueInspect,
    structured_storage::{
        memory::InMemoryStorage,
        StructuredStorage,
    },
    tables::{
        Coins,
        ConsensusParametersVersions,
        ContractsLatestUtxo,
        FuelBlocks,
    },
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        HistoricalView,
        Modifiable,
        StorageTransaction,
    },
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        header::BlockHeaderError,
    },
    fuel_tx::{
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        ConsensusParameters,
        Input,
        TxPointer,
    },
    fuel_types::BlockHeight,
    fuel_vm::interpreter::MemoryInstance,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            ProductionResult,
            Result as ExecutorResult,
            UncommittedResult,
            ValidationResult,
        },
        Uncommitted,
    },
};
use fuel_core_upgradable_executor::{
    executor::Executor as UpgradableExecutor,
    native_executor,
    native_executor::{
        executor::{
            max_tx_count,
            ExecutionData,
            ExecutionOptions,
            ExecutionResult,
            OnceTransactionsSource,
        },
        ports::{
            RelayerPort,
            TransactionExt,
            TransactionsSource,
        },
    },
};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        RwLock,
    },
};
use tokio::runtime::Runtime;

pub struct Executor<S, R> {
    executor: Arc<RwLock<UpgradableExecutor<S, R>>>,
    runtime: Runtime,
    number_of_cores: NonZeroUsize,
}
// TODO: Shutdown the tokio runtime to avoid panic if executor is already
//   used from another tokio runtime
// impl<S, R> Drop for Executor<S, R> {
//     fn drop(&mut self) {
//         self.runtime.shutdown_background();
//     }
// }

impl<S, R> Executor<S, R> {
    pub fn new(
        storage_view_provider: S,
        relayer_view_provider: R,
        config: Config,
    ) -> Self {
        let executor = UpgradableExecutor::new(
            storage_view_provider,
            relayer_view_provider,
            config.executor_config,
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.number_of_cores.get())
            .enable_all()
            .build()
            .unwrap();
        let number_of_cores = config.number_of_cores;

        Self {
            executor: Arc::new(RwLock::new(executor)),
            runtime,
            number_of_cores,
        }
    }
}

impl<S, R> Executor<S, R>
where
    S: HistoricalView<Height = BlockHeight> + Modifiable + 'static,
    S::LatestView: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    S::ViewAtHeight: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView + 'static,
    R::LatestView: RelayerPort + Send + Sync + 'static,
{
    #[cfg(feature = "test-helpers")]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn produce_and_commit(
        &mut self,
        block: PartialFuelBlock,
    ) -> fuel_core_types::services::executor::Result<ProductionResult> {
        let (result, changes) = self.produce_without_commit(block)?.into();

        let mut executor = self.executor.write().map_err(|e| {
            ExecutorError::Other(format!(
                "Unable to get the write lock for the executor: {e}"
            ))
        })?;

        executor.storage_view_provider.commit_changes(changes)?;
        Ok(result)
    }

    #[cfg(feature = "test-helpers")]
    /// Executes the block and commits the result of the execution into the inner `Database`.
    pub fn validate_and_commit(
        &mut self,
        block: &Block,
    ) -> fuel_core_types::services::executor::Result<ValidationResult> {
        let (result, changes) = self.validate(block)?.into();

        let mut executor = self.executor.write().map_err(|e| {
            ExecutorError::Other(format!(
                "Unable to get the write lock for the executor: {e}"
            ))
        })?;
        executor.storage_view_provider.commit_changes(changes)?;

        Ok(result)
    }
}

#[cfg(feature = "test-helpers")]
impl<S, R> Executor<S, R>
where
    S: HistoricalView<Height = BlockHeight> + 'static,
    S::LatestView: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    S::ViewAtHeight: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView + 'static,
    R::LatestView: RelayerPort + Send + Sync + 'static,
{
    /// Executes the block and returns the result of the execution with storage changes.
    pub fn produce_without_commit(
        &self,
        block: PartialFuelBlock,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        self.produce_without_commit_with_coinbase(block, Default::default(), 0)
    }

    /// The analog of the [`Self::produce_without_commit`] method,
    /// but with the ability to specify the coinbase recipient and the gas price.
    pub fn produce_without_commit_with_coinbase(
        &self,
        block: PartialFuelBlock,
        coinbase_recipient: fuel_core_types::fuel_types::ContractId,
        gas_price: u64,
    ) -> fuel_core_types::services::executor::Result<UncommittedResult<Changes>> {
        let component = Components {
            header_to_produce: block.header,
            transactions_source: OnceTransactionsSource::new(block.transactions),
            coinbase_recipient,
            gas_price,
        };

        self.produce_inner(component)
    }

    /// Executes a dry-run of the block and returns the result of the execution without committing the changes.
    pub fn dry_run_without_commit_with_source<TxSource>(
        &self,
        block: Components<TxSource>,
    ) -> ExecutorResult<ExecutionResult>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let executor = self.executor.read().map_err(|e| {
            ExecutorError::Other(format!(
                "Unable to get the read lock for the executor: {e}"
            ))
        })?;
        executor.dry_run_without_commit_with_source(block)
    }

    pub fn get_executor(&self) -> Arc<RwLock<UpgradableExecutor<S, R>>> {
        self.executor.clone()
    }
}

impl<S, R> Executor<S, R>
where
    S: HistoricalView<Height = BlockHeight> + 'static,
    S::LatestView: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    S::ViewAtHeight: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    R: AtomicView + 'static,
    R::LatestView: RelayerPort + Send + Sync + 'static,
{
    /// Produces the block and returns the result of the execution without committing the changes.
    pub fn produce_without_commit_with_source<TxSource>(
        &self,
        components: Components<TxSource>,
    ) -> ExecutorResult<Uncommitted<ProductionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        self.produce_inner(components)
    }

    /// Executes the block and returns the result of the execution without committing
    /// the changes in the dry run mode.
    pub fn dry_run<TxSource>(
        &self,
        component: Components<TxSource>,
        utxo_validation: Option<bool>,
        allow_historical: bool,
    ) -> ExecutorResult<ExecutionResult>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let executor = self.executor.read().map_err(|e| {
            ExecutorError::Other(format!(
                "Unable to get the read lock for the executor: {e}"
            ))
        })?;
        executor.dry_run(component, utxo_validation, allow_historical)
    }

    pub fn validate(
        &self,
        block: &Block,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        self.validate_inner(block)
    }

    fn produce_inner<TxSource>(
        &self,
        components: Components<TxSource>,
    ) -> ExecutorResult<Uncommitted<ProductionResult, Changes>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let prev_height = components
            .header_to_produce
            .height()
            .pred()
            .ok_or(ExecutorError::PreviousBlockIsNotFound)?;
        let executor = self.executor.read().map_err(|e| {
            ExecutorError::Other(format!(
                "Unable to get the read lock for the executor: {e}"
            ))
        })?;

        let execution_options = executor.config.as_ref().into();
        let view =
            StructuredStorage::new(executor.storage_view_provider.view_at(&prev_height)?);

        let previous_block = view
            .storage_as_ref::<FuelBlocks>()
            .get(&prev_height)?
            .ok_or(ExecutorError::PreviousBlockIsNotFound)?;

        let consensus_parameters_version =
            components.header_to_produce.consensus_parameters_version;

        let consensus_parameters = view
            .storage_as_ref::<ConsensusParametersVersions>()
            .get(&consensus_parameters_version)?
            .ok_or(ExecutorError::ConsensusParametersNotFound(
                consensus_parameters_version,
            ))?;
        let block_gas_limit = consensus_parameters.block_gas_limit();

        // TODO: Support parallel execution when the block is increased.
        //  We can always process DA first, and on top of this result run transactions in parallel.
        if previous_block.header().da_height != components.header_to_produce.da_height {
            let new_small_block = Components {
                header_to_produce: components.header_to_produce,
                transactions_source: OneCoreTxSource::new(
                    components.transactions_source,
                    block_gas_limit,
                    self.number_of_cores,
                ),
                coinbase_recipient: components.coinbase_recipient,
                gas_price: components.gas_price,
            };

            return executor.produce_without_commit_with_source(new_small_block)
        }

        let max_tx_number = max_tx_count();
        let block_size_limit =
            u32::try_from(consensus_parameters.block_transaction_size_limit())
                .unwrap_or(u32::MAX);

        let Components {
            header_to_produce,
            transactions_source,
            coinbase_recipient,
            gas_price,
        } = components;

        let txs =
            transactions_source.next(block_gas_limit, max_tx_number, block_size_limit);

        let chain_id = consensus_parameters.chain_id();
        let consensus_parameters = consensus_parameters.into_owned();
        // TODO: Use reference for `consensus_parameters`.
        let mut splitter = DependencySplitter::new(consensus_parameters.clone());

        let mut skipped_transactions = vec![];

        for tx in txs {
            let tx_id = tx.id(&chain_id);
            let result = splitter.process(tx);

            if let Err(err) = result {
                skipped_transactions.push((tx_id, err));
            }
        }

        let buckets = splitter.split_equally(self.number_of_cores);

        let handlers = buckets
            .into_iter()
            .map(|txs| {
                let part_of_the_block = Components {
                    header_to_produce,
                    transactions_source: OnceTransactionsSource::new_maybe_checked(txs),
                    coinbase_recipient,
                    gas_price,
                };
                let executor = self.executor.clone();
                self.runtime.spawn(async move {
                    let executor = executor.read().map_err(|e| {
                        ExecutorError::Other(format!(
                            "Unable to get the read lock for the executor: {e}"
                        ))
                    })?;
                    let allow_historical = true;
                    executor.dry_run(part_of_the_block, None, allow_historical)
                })
            })
            .collect::<Vec<_>>();

        let results = self
            .runtime
            .block_on(async move { futures::future::join_all(handlers).await });

        let execution_results = results
            .into_iter()
            .map(|result| {
                let uncommited_result = result.map_err(|e| {
                    ExecutorError::Other(format!(
                        "Unable to join one of the executors {e}"
                    ))
                })??;
                Ok(uncommited_result)
            })
            .collect::<ExecutorResult<Vec<_>>>()?;

        let components = Components {
            header_to_produce,
            transactions_source: (),
            coinbase_recipient,
            gas_price,
        };

        let result = merge_execution_results(
            components,
            consensus_parameters,
            execution_options,
            execution_results,
        )?;

        #[cfg(debug_assertions)]
        {
            let components = Components {
                header_to_produce,
                transactions_source: OnceTransactionsSource::new(
                    result
                        .result()
                        .block
                        .transactions()
                        .split_last()
                        .ok_or(ExecutorError::MintMismatch)?
                        .1
                        .to_vec(),
                ),
                coinbase_recipient,
                gas_price,
            };
            let old_result = executor.produce_without_commit_with_source(components)?;
            assert_eq!(old_result.changes(), result.changes());
            assert_eq!(old_result.result(), result.result());
        }

        Ok(result)
    }

    fn validate_inner(
        &self,
        block: &Block,
    ) -> ExecutorResult<Uncommitted<ValidationResult, Changes>> {
        let (gas_price, coinbase_contract_id) = native_executor::executor::BlockExecutor::<
            (),
        >::get_coinbase_info_from_mint_tx(
            block.transactions()
        )?;

        let components = Components {
            header_to_produce: block.header().into(),
            transactions_source: OnceTransactionsSource::new(
                block
                    .transactions()
                    .split_last()
                    .ok_or(ExecutorError::MintMissing)?
                    .1
                    .to_vec(),
            ),
            coinbase_recipient: coinbase_contract_id,
            gas_price,
        };
        let executed_block_result = self.produce_inner(components)?;

        dbg!(&executed_block_result.result().block.id());
        dbg!(&block.id());
        dbg!(&executed_block_result.result().block);
        dbg!(&block);
        if &executed_block_result.result().block == block {
            Ok(executed_block_result.into_validation_result())
        } else {
            Err(ExecutorError::BlockMismatch)
        }
    }
}

fn merge_execution_results(
    components: Components<()>,
    consensus_parameters: ConsensusParameters,
    options: ExecutionOptions,
    uncommited_results: Vec<ExecutionResult>,
) -> ExecutorResult<UncommittedResult<Changes>> {
    let dummy_view = InMemoryStorage::default();
    let partial_block_header = components.header_to_produce;

    let mut txs_count = 0usize;
    let mut events_count = 0usize;
    let mut skipped_count = 0usize;

    for part in uncommited_results.iter() {
        let tx_count_with_mint = part.partial_block.transactions.len();
        txs_count = txs_count.saturating_add(tx_count_with_mint);

        let events_with_mint = part.execution_data.events.len();
        events_count = events_count.saturating_add(events_with_mint);

        let skipped = part.execution_data.skipped_transactions.len();
        skipped_count = skipped_count.saturating_add(skipped);
    }

    let mut total_partial_block = PartialFuelBlock {
        header: partial_block_header,
        transactions: Vec::with_capacity(txs_count),
    };

    let mut total_data = ExecutionData {
        coinbase: 0,
        used_gas: 0,
        used_size: 0,
        tx_count: 0,
        found_mint: false,
        message_ids: vec![],
        tx_status: Vec::with_capacity(txs_count),
        events: Vec::with_capacity(events_count),
        changes: Default::default(),
        skipped_transactions: Vec::with_capacity(skipped_count),
        event_inbox_root: ExecutionData::default_event_inbox_root(),
    };

    let mut total_changes = StorageTransaction::transaction(
        &dummy_view,
        ConflictPolicy::Fail,
        Changes::default(),
    );

    let current_height = *total_partial_block.header.height();

    for part in uncommited_results {
        let ExecutionResult {
            partial_block: mut part_block,
            execution_data,
        } = part;

        total_data.coinbase = total_data
            .coinbase
            .checked_add(execution_data.coinbase)
            .ok_or(ExecutorError::FeeOverflow)?;
        total_data.used_gas = total_data
            .used_gas
            .checked_add(execution_data.used_gas)
            .ok_or(ExecutorError::GasOverflow(
                "Execution used gas overflowed.".into(),
                total_data.used_gas,
                execution_data.used_gas,
            ))?;
        total_data.used_size = total_data
            .used_size
            .checked_add(execution_data.used_size)
            .ok_or(ExecutorError::TxSizeOverflow)?;

        debug_assert_eq!(part_block.header, total_partial_block.header);

        let tx_index_offset = u32::try_from(total_partial_block.transactions.len())
            .map_err(|_| {
                ExecutorError::BlockHeaderError(BlockHeaderError::TooManyTransactions)
            })?;

        for tx in part_block.transactions.iter_mut() {
            let inputs = tx.inputs_mut()?;

            for input in inputs {
                match input {
                    Input::CoinSigned(CoinSigned { tx_pointer, .. })
                    | Input::CoinPredicate(CoinPredicate { tx_pointer, .. }) => {
                        debug_assert_eq!(
                            tx_pointer.block_height(),
                            *total_partial_block.header.height()
                        );

                        if tx_pointer.block_height() == current_height {
                            let new_tx_index = tx_pointer
                                .tx_index()
                                .checked_add(tx_index_offset)
                                .ok_or(ExecutorError::BlockHeaderError(
                                    BlockHeaderError::TooManyTransactions,
                                ))?;
                            let new_tx_pointer =
                                TxPointer::new(current_height, new_tx_index);

                            *tx_pointer = new_tx_pointer;
                        }
                    }
                    Input::Contract(contract) => {
                        let tx_pointer = &mut contract.tx_pointer;

                        debug_assert_eq!(
                            tx_pointer.block_height(),
                            *total_partial_block.header.height()
                        );

                        if tx_pointer.block_height() == current_height {
                            let new_tx_index = tx_pointer
                                .tx_index()
                                .checked_add(tx_index_offset)
                                .ok_or(ExecutorError::BlockHeaderError(
                                    BlockHeaderError::TooManyTransactions,
                                ))?;
                            let new_tx_pointer =
                                TxPointer::new(current_height, new_tx_index);

                            *tx_pointer = new_tx_pointer;
                        }
                    }
                    Input::MessageCoinSigned(_)
                    | Input::MessageCoinPredicate(_)
                    | Input::MessageDataSigned(_)
                    | Input::MessageDataPredicate(_) => {
                        // Nothing to update
                    }
                }
            }
        }

        let changes = execution_data.changes;
        let mut changes_storage_tx = StorageTransaction::transaction(
            &dummy_view,
            ConflictPolicy::Overwrite,
            Changes::default(),
        );

        let iter = ChangesIterator::<Column>::new(&changes);
        let coins_iter = iter.iter_all::<Coins>(None);
        let contracts_iter = iter.iter_all::<ContractsLatestUtxo>(None);

        for result in coins_iter {
            let (utxo_id, mut coin) = result?;

            let tx_pointer = *coin.tx_pointer();

            if tx_pointer.block_height() == current_height {
                let new_tx_index = tx_pointer
                    .tx_index()
                    .checked_add(tx_index_offset)
                    .ok_or(ExecutorError::BlockHeaderError(
                        BlockHeaderError::TooManyTransactions,
                    ))?;
                let new_tx_pointer = TxPointer::new(current_height, new_tx_index);

                coin.set_tx_pointer(new_tx_pointer);

                changes_storage_tx
                    .storage_as_mut::<Coins>()
                    .insert(&utxo_id, &coin)?;
            }
        }

        for result in contracts_iter {
            let (contract_id, mut info) = result?;

            let tx_pointer = info.tx_pointer();

            if tx_pointer.block_height() == current_height {
                let new_tx_index = tx_pointer
                    .tx_index()
                    .checked_add(tx_index_offset)
                    .ok_or(ExecutorError::BlockHeaderError(
                        BlockHeaderError::TooManyTransactions,
                    ))?;
                let new_tx_pointer = TxPointer::new(current_height, new_tx_index);

                info.set_tx_pointer(new_tx_pointer);

                changes_storage_tx
                    .storage_as_mut::<ContractsLatestUtxo>()
                    .insert(&contract_id, &info)?;
            }
        }

        let shifted_changes = changes_storage_tx.into_changes();

        let mut final_storage_tx = StorageTransaction::transaction(
            &dummy_view,
            ConflictPolicy::Overwrite,
            changes,
        );
        final_storage_tx.commit_changes(shifted_changes)?;

        let part_changes = final_storage_tx.into_changes();

        // If `part_changes` to the database gas conflicts, `commit_changes` should fail.
        total_changes.commit_changes(part_changes)?;

        total_data.tx_status.extend(execution_data.tx_status);
        total_data.events.extend(execution_data.events);
        total_data
            .skipped_transactions
            .extend(execution_data.skipped_transactions);
        total_data.message_ids.extend(execution_data.message_ids);

        total_partial_block
            .transactions
            .extend(part_block.transactions);
        total_data.tx_count = u32::try_from(total_partial_block.transactions.len())
            .map_err(|_| {
                ExecutorError::BlockHeaderError(BlockHeaderError::TooManyTransactions)
            })?;
    }

    let block_executor =
        native_executor::executor::BlockExecutor::new((), options, consensus_parameters);

    let mut memory = MemoryInstance::new();
    block_executor.produce_mint_tx(
        &mut total_partial_block,
        &components,
        &mut total_changes,
        &mut total_data,
        &mut memory,
    )?;

    total_data.changes = total_changes.into_changes();

    let ExecutionData {
        message_ids,
        event_inbox_root,
        changes,
        events,
        tx_status,
        skipped_transactions,
        coinbase,
        used_gas,
        used_size,
        ..
    } = total_data;

    let block = total_partial_block
        .generate(&message_ids[..], event_inbox_root)
        .map_err(ExecutorError::BlockHeaderError)?;

    let finalized_block_id = block.id();

    tracing::debug!(
        "Block {:#x} fees: {} gas: {} tx_size: {}",
        finalized_block_id,
        coinbase,
        used_gas,
        used_size
    );

    let result = ProductionResult {
        block,
        skipped_transactions,
        tx_status,
        events,
    };

    Ok(UncommittedResult::new(result, changes))
}

#[cfg(test)]
mod tests {
    use fuel_core_storage::{
        column::Column,
        kv_store::{
            KeyValueInspect,
            Value,
        },
        structured_storage::memory::InMemoryStorage,
        tables::{
            ConsensusParametersVersions,
            FuelBlocks,
        },
        transactional::{
            AtomicView,
            Changes,
            HistoricalView,
            Modifiable,
            WriteTransaction,
        },
        Result as StorageResult,
        StorageAsMut,
    };
    use fuel_core_types::{
        blockchain::{
            block::{
                Block,
                PartialFuelBlock,
            },
            header::{
                ApplicationHeader,
                ConsensusHeader,
                PartialBlockHeader,
                StateTransitionBytecodeVersion,
                LATEST_STATE_TRANSITION_VERSION,
            },
            primitives::{
                DaBlockHeight,
                Empty,
            },
        },
        fuel_tx::{
            AssetId,
            Cacheable,
            Transaction,
            TxPointer,
        },
        fuel_types::{
            BlockHeight,
            ChainId,
        },
        services::relayer::Event,
        tai64::Tai64,
    };
    use fuel_core_upgradable_executor::native_executor::{
        executor::ExecutionData,
        ports::RelayerPort,
    };

    use crate::executor::Executor;

    #[derive(Clone, Debug)]
    struct Storage(InMemoryStorage<Column>);

    impl AtomicView for Storage {
        type LatestView = InMemoryStorage<Column>;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(self.0.clone())
        }
    }

    impl HistoricalView for Storage {
        type Height = BlockHeight;
        type ViewAtHeight = Self::LatestView;

        fn latest_height(&self) -> Option<Self::Height> {
            None
        }

        fn view_at(&self, _: &Self::Height) -> StorageResult<Self::ViewAtHeight> {
            self.latest_view()
        }
    }

    impl KeyValueInspect for Storage {
        type Column = Column;

        fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
            self.0.get(key, column)
        }
    }

    impl Modifiable for Storage {
        fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
            self.0.commit_changes(changes)
        }
    }

    #[derive(Copy, Clone)]
    struct DisabledRelayer;

    impl RelayerPort for DisabledRelayer {
        fn enabled(&self) -> bool {
            false
        }

        fn get_events(&self, _: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
            unimplemented!()
        }
    }

    impl AtomicView for DisabledRelayer {
        type LatestView = Self;

        fn latest_view(&self) -> StorageResult<Self::LatestView> {
            Ok(*self)
        }
    }

    const CONSENSUS_PARAMETERS_VERSION: u32 = 0;

    fn storage() -> Storage {
        let mut storage = Storage(InMemoryStorage::default());
        let mut tx = storage.write_transaction();
        tx.storage_as_mut::<ConsensusParametersVersions>()
            .insert(&CONSENSUS_PARAMETERS_VERSION, &Default::default())
            .unwrap();
        tx.commit().unwrap();

        storage
    }

    fn valid_block(
        storage: &mut Storage,
        state_transition_bytecode_version: StateTransitionBytecodeVersion,
        mut transactions: Vec<Transaction>,
    ) -> Block {
        let prev_block = PartialFuelBlock::new(
            PartialBlockHeader {
                application: ApplicationHeader {
                    da_height: Default::default(),
                    consensus_parameters_version: CONSENSUS_PARAMETERS_VERSION,
                    state_transition_bytecode_version,
                    generated: Empty,
                },
                consensus: ConsensusHeader {
                    prev_root: Default::default(),
                    height: BlockHeight::new(0),
                    time: Tai64::now(),
                    generated: Empty,
                },
            },
            vec![Transaction::mint(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                AssetId::BASE,
                Default::default(),
            )
            .into()],
        )
        .generate(&[], ExecutionData::default_event_inbox_root())
        .unwrap();
        let mut tx = storage.write_transaction();
        tx.storage_as_mut::<FuelBlocks>()
            .insert(
                &BlockHeight::new(0),
                &prev_block.compress(&ChainId::default()),
            )
            .unwrap();
        tx.commit().unwrap();
        let mut mint_tx = Transaction::mint(
            TxPointer::new(BlockHeight::new(1), 0),
            Default::default(),
            Default::default(),
            Default::default(),
            AssetId::BASE,
            Default::default(),
        );
        mint_tx.precompute(&ChainId::default()).unwrap();
        transactions.push(mint_tx.into());
        PartialFuelBlock::new(
            PartialBlockHeader {
                application: ApplicationHeader {
                    da_height: Default::default(),
                    consensus_parameters_version: CONSENSUS_PARAMETERS_VERSION,
                    state_transition_bytecode_version,
                    generated: Empty,
                },
                consensus: ConsensusHeader {
                    prev_root: prev_block.id().into(),
                    height: BlockHeight::new(1),
                    time: Tai64::now(),
                    generated: Empty,
                },
            },
            transactions,
        )
        .generate(&[], ExecutionData::default_event_inbox_root())
        .unwrap()
    }

    #[test]
    fn can_validate_block() {
        let mut storage = storage();

        // Given
        let block = valid_block(&mut storage, LATEST_STATE_TRANSITION_VERSION, vec![]);
        let executor = Executor::new(storage, DisabledRelayer, Default::default());

        // When
        let result = executor.validate(&block).map(|_| ());

        // Then
        assert_eq!(Ok(()), result);
    }
}
