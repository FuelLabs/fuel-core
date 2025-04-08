use crate::{
    error::Error,
    local_runner::LocalRunner,
    ports::{
        BlockVerifier,
        DatabaseTransaction,
        ImporterDatabase,
        Transactional,
        Validator,
    },
    Config,
    ImporterResult,
};
use fuel_core_metrics::importer::importer_metrics;
use fuel_core_storage::{
    not_found,
    transactional::{
        Changes,
        StorageChanges,
    },
};
use fuel_core_types::{
    blockchain::{
        consensus::Consensus,
        SealedBlock,
    },
    fuel_tx::{
        field::MintGasPrice,
        Transaction,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    services::{
        block_importer::{
            ImportResult,
            UncommittedResult,
        },
        executor::{
            Event,
            TransactionExecutionStatus,
            ValidationResult,
        },
    },
};
use std::{
    ops::Deref,
    sync::Arc,
    time::{
        Duration,
        Instant,
        SystemTime,
        UNIX_EPOCH,
    },
};
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot,
    OwnedSemaphorePermit,
    Semaphore,
};
use tracing::warn;

#[cfg(test)]
pub mod test;

enum CommitInput {
    Uncommitted(UncommittedResult<Changes>),
    PrepareImportResult(PrepareImportResult),
}

enum Commands {
    Stop,
    CommitResult {
        result: CommitInput,
        permit: OwnedSemaphorePermit,
        callback: oneshot::Sender<Result<(), Error>>,
    },
    #[cfg(test)]
    VerifyAndExecuteBlock {
        sealed_block: SealedBlock,
        callback: oneshot::Sender<Result<UncommittedResult<Changes>, Error>>,
    },
    PrepareImportResult {
        sealed_block: SealedBlock,
        callback: oneshot::Sender<Result<PrepareImportResult, Error>>,
    },
}

struct ImporterInner<D, E, V> {
    database: D,
    executor: E,
    verifier: V,
    chain_id: ChainId,
    broadcast: broadcast::Sender<ImporterResult>,
    commands: mpsc::UnboundedReceiver<Commands>,
    /// Enables prometheus metrics for this fuel-service
    metrics: bool,
}

pub struct Importer {
    broadcast: broadcast::Sender<ImporterResult>,
    guard: Semaphore,
    commands: mpsc::UnboundedSender<Commands>,
    /// The semaphore tracks the number of unprocessed `SharedImportResult`.
    /// If the number of unprocessed results is more than the threshold,
    /// the block importer stops committing new blocks and waits for
    /// the resolution of the previous one.
    active_import_results: Arc<Semaphore>,
    inner: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Importer {
    fn drop(&mut self) {
        let _ = self.commands.send(Commands::Stop);

        let inner = self.inner.take();

        if let Some(inner) = inner {
            let result = inner.join();

            if let Err(err) = result {
                tracing::error!("The inner importer thread panicked: {:?}", err);
            }
        }
    }
}

impl Importer {
    pub fn new<D, E, V>(
        chain_id: ChainId,
        config: Config,
        database: D,
        executor: E,
        verifier: V,
    ) -> Self
    where
        D: ImporterDatabase + Transactional + 'static,
        E: Validator + 'static,
        V: BlockVerifier + 'static,
    {
        // We use semaphore as a back pressure mechanism instead of a `broadcast`
        // channel because we want to prevent committing to the database results
        // that will not be processed.
        let max_block_notify_buffer = config.max_block_notify_buffer;
        let (broadcast, _) = broadcast::channel(max_block_notify_buffer);
        let (sender, receiver) = mpsc::unbounded_channel();

        let mut inner = ImporterInner {
            database,
            executor,
            verifier,
            chain_id,
            commands: receiver,
            broadcast: broadcast.clone(),
            metrics: config.metrics,
        };

        if config.metrics {
            inner.init_metrics();
        }

        let inner = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("Failed to create tokio runtime for importer inner task");
            runtime.block_on(inner.run());
        });

        Self {
            broadcast,
            commands: sender,
            active_import_results: Arc::new(Semaphore::new(max_block_notify_buffer)),
            guard: Semaphore::new(1),
            inner: Some(inner),
        }
    }

    #[cfg(test)]
    pub fn default_config<D, E, V>(database: D, executor: E, verifier: V) -> Self
    where
        D: ImporterDatabase + Transactional + 'static,
        E: Validator + 'static,
        V: BlockVerifier + 'static,
    {
        Self::new(
            Default::default(),
            Default::default(),
            database,
            executor,
            verifier,
        )
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ImporterResult> {
        self.broadcast.subscribe()
    }

    pub(crate) fn lock(&self) -> Result<tokio::sync::SemaphorePermit, Error> {
        let guard = self.guard.try_acquire();
        match guard {
            Ok(permit) => Ok(permit),
            Err(err) => {
                tracing::error!(
                    "The semaphore was acquired before. It is a problem \
                    because the current architecture doesn't expect that."
                );
                Err(Error::Semaphore(err))
            }
        }
    }
}

impl Importer {
    /// The method commits the result of the block execution attaching the consensus data.
    /// It expects that the `UncommittedResult` contains the result of the block
    /// execution(It includes the block itself), but not more.
    ///
    /// It doesn't do any checks regarding block validity(execution, fields, signing, etc.).
    /// It only checks the validity of the database.
    ///
    /// After the commit into the database notifies about a new imported block.
    ///
    /// # Concurrency
    ///
    /// Only one commit may be in progress at the time. All other calls will fail.
    /// Returns an error if called while another call is in progress.
    pub async fn commit_result(
        &self,
        result: UncommittedResult<Changes>,
    ) -> Result<(), Error> {
        let _guard = self.lock()?;

        // Await until all receivers of the notification process the result.
        const TIMEOUT: u64 = 20;
        let await_result = tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            self.active_import_results.clone().acquire_owned(),
        )
        .await;

        let Ok(permit) = await_result else {
            tracing::error!(
                "The previous block processing \
                    was not finished for {TIMEOUT} seconds."
            );
            return Err(Error::PreviousBlockProcessingNotFinished)
        };
        let permit = permit.map_err(Error::ActiveBlockResultsSemaphoreClosed)?;

        self.run_commit_result(permit, CommitInput::Uncommitted(result))
            .await
    }

    async fn run_commit_result(
        &self,
        permit: OwnedSemaphorePermit,
        result: CommitInput,
    ) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();
        let command = Commands::CommitResult {
            result,
            permit,
            callback: sender,
        };
        self.commands.send(command)?;
        receiver.await?
    }

    #[cfg(test)]
    async fn run_verify_and_execute_block(
        &self,
        sealed_block: SealedBlock,
    ) -> Result<UncommittedResult<Changes>, Error> {
        let (sender, receiver) = oneshot::channel();
        let command = Commands::VerifyAndExecuteBlock {
            sealed_block,
            callback: sender,
        };
        self.commands.send(command)?;
        receiver.await?
    }

    async fn run_prepare_import_result(
        &self,
        sealed_block: SealedBlock,
    ) -> Result<PrepareImportResult, Error> {
        let (sender, receiver) = oneshot::channel();
        let command = Commands::PrepareImportResult {
            sealed_block,
            callback: sender,
        };
        self.commands.send(command)?;
        receiver.await?
    }
}

impl<D, E, V> ImporterInner<D, E, V>
where
    D: ImporterDatabase + Transactional,
    E: Send + Sync,
    V: Send + Sync,
{
    /// The method commits the result of the block execution attaching the consensus data.
    /// It expects that the `UncommittedResult` contains the result of the block
    /// execution(It includes the block itself), but not more.
    ///
    /// It doesn't do any checks regarding block validity(execution, fields, signing, etc.).
    /// It only checks the validity of the database.
    ///
    /// After the commit into the database notifies about a new imported block.
    pub fn commit_result(
        &mut self,
        runner: &LocalRunner,
        permit: OwnedSemaphorePermit,
        result: CommitInput,
    ) -> Result<(), Error> {
        runner.run(move || {
            let result = match result {
                CommitInput::Uncommitted(result) => {
                    let block_changes = create_block_changes(
                        &self.chain_id,
                        &result.result().sealed_block,
                        &self.database,
                    )?;

                    PrepareImportResult {
                        result,
                        block_changes,
                    }
                }
                CommitInput::PrepareImportResult(result) => result,
            };

            self._commit_result(result, permit)
        })
    }

    /// The method commits the result of the block execution and notifies about a new imported block.
    #[tracing::instrument(
        skip_all,
        fields(
            block_id = % prepare.result.result().sealed_block.entity.id(),
            height = * * prepare.result.result().sealed_block.entity.header().height(),
            tx_status = ? prepare.result.result().tx_status,
        ),
        err
    )]
    fn _commit_result(
        &mut self,
        prepare: PrepareImportResult,
        permit: OwnedSemaphorePermit,
    ) -> Result<(), Error> {
        let PrepareImportResult {
            result,
            block_changes,
        } = prepare;

        let (result, changes) = result.into();
        let block = &result.sealed_block.entity;
        let actual_next_height = *block.header().height();

        // Importer expects that `UncommittedResult` contains the result of block
        // execution without block itself.
        let expected_block_root = self.database.latest_block_root()?;

        let db_after_execution = self.database.storage_transaction(changes);
        let actual_block_root = db_after_execution.latest_block_root()?;

        if actual_block_root != expected_block_root {
            return Err(Error::InvalidDatabaseStateAfterExecution(
                expected_block_root,
                actual_block_root,
            ))
        }

        let changes = db_after_execution.into_changes();

        #[cfg(feature = "test-helpers")]
        let changes_clone = changes.clone();

        self.database
            .commit_changes(StorageChanges::ChangesList(vec![block_changes, changes]))?;

        if self.metrics {
            Self::update_metrics(&result, &actual_next_height);
        }
        tracing::info!("Committed block {:#x}", result.sealed_block.entity.id());

        let result = ImporterResult {
            shared_result: Arc::new(Awaiter::new(result, permit)),
            #[cfg(feature = "test-helpers")]
            changes: Arc::new(changes_clone),
        };
        let _ = self.broadcast.send(result);

        Ok(())
    }

    /// Should only be called once after startup to set importer metrics to their initial values
    pub fn init_metrics(&self) {
        // load starting values from database

        // Errors are optimistically handled via fallback to default values since the metrics
        // should get updated regularly anyways and these errors will be discovered and handled
        // correctly in more mission critical areas (such as _commit_result)
        let current_block_height = self
            .database
            .latest_block_height()
            .unwrap_or_default()
            .unwrap_or_default();
        importer_metrics()
            .block_height
            .set(*current_block_height.deref() as i64);
        // on init just set to current time since it's not worth tracking in the db
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        importer_metrics()
            .latest_block_import_timestamp
            .set(current_time);
    }

    fn update_metrics(result: &ImportResult, actual_next_height: &BlockHeight) {
        let (total_gas_used, total_fee): (u64, u64) = result
            .tx_status
            .iter()
            .map(|tx_result| {
                (*tx_result.result.total_gas(), *tx_result.result.total_fee())
            })
            .fold((0_u64, 0_u64), |(acc_gas, acc_fee), (used_gas, fee)| {
                (
                    acc_gas.saturating_add(used_gas),
                    acc_fee.saturating_add(fee),
                )
            });
        let maybe_last_tx = result.sealed_block.entity.transactions().last();
        if let Some(last_tx) = maybe_last_tx {
            if let Transaction::Mint(mint) = last_tx {
                importer_metrics()
                    .gas_price
                    .set((*mint.gas_price()).try_into().unwrap_or(i64::MAX));
            } else {
                warn!("Last transaction is not a mint transaction");
            }
        }

        let total_transactions = result.tx_status.len();
        importer_metrics()
            .block_height
            .set(*actual_next_height.deref() as i64);
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        importer_metrics()
            .latest_block_import_timestamp
            .set(current_time);
        importer_metrics()
            .gas_per_block
            .set(total_gas_used.try_into().unwrap_or(i64::MAX));
        importer_metrics()
            .fee_per_block
            .set(total_fee.try_into().unwrap_or(i64::MAX));
        importer_metrics()
            .transactions_per_block
            .set(total_transactions.try_into().unwrap_or(i64::MAX));
    }
}

struct VerifyAndExecutionResult {
    tx_status: Vec<TransactionExecutionStatus>,
    events: Vec<Event>,
    changes: Changes,
}

struct PrepareImportResult {
    result: UncommittedResult<Changes>,
    block_changes: Changes,
}

impl<IDatabase, E, V> ImporterInner<IDatabase, E, V>
where
    IDatabase: ImporterDatabase + Transactional,
    E: Validator,
    V: BlockVerifier,
{
    async fn run(&mut self) {
        let local_runner = LocalRunner::new().expect("Failed to create the local runner");
        while let Some(command) = self.commands.recv().await {
            match command {
                Commands::Stop => break,
                Commands::CommitResult {
                    result,
                    permit,
                    callback,
                } => {
                    let result = self.commit_result(&local_runner, permit, result);
                    let _ = callback.send(result);
                }
                #[cfg(test)]
                Commands::VerifyAndExecuteBlock {
                    sealed_block,
                    callback,
                } => {
                    let result =
                        self.verify_and_execute_block(&local_runner, sealed_block);
                    let _ = callback.send(result);
                }
                Commands::PrepareImportResult {
                    sealed_block,
                    callback,
                } => {
                    let result = self.prepare_import_result(&local_runner, sealed_block);
                    let _ = callback.send(result);
                }
            }
        }
    }

    /// Prepares the block for committing. It includes the execution of the block,
    /// the validation of the block fields, and preparing the changes
    /// to the database with the block and transactions.
    pub fn prepare_import_result(
        &self,
        runner: &LocalRunner,
        sealed_block: SealedBlock,
    ) -> Result<PrepareImportResult, Error> {
        let sealed_block_ref = &sealed_block;

        let block_changes =
            || create_block_changes(&self.chain_id, sealed_block_ref, &self.database);

        let execution = || {
            Self::verify_and_execute_block_inner(
                &self.executor,
                &self.verifier,
                sealed_block_ref,
            )
        };

        let (block_changes_result, execution_result) =
            runner.run_in_parallel(block_changes, execution);

        let result = execution_result?;
        let block_changes = block_changes_result?;

        let VerifyAndExecutionResult {
            tx_status,
            events,
            changes,
        } = result;
        let import_result =
            ImportResult::new_from_network(sealed_block, tx_status, events);

        let result = UncommittedResult::new(import_result, changes);

        Ok(PrepareImportResult {
            result,
            block_changes,
        })
    }

    /// Performs all checks required to commit the block, it includes the execution of
    /// the block(As a result returns the uncommitted state).
    ///
    /// It validates only the `Block` execution rules and the `Block` fields' validity.
    /// The validity of the `SealedBlock` and seal information is not the concern of this function.
    ///
    /// The method doesn't require synchronous access, so it could be called in a
    /// concurrent environment.
    ///
    /// Returns `Err` if the block is invalid for committing. Otherwise, it returns the
    /// `Ok` with the uncommitted state.
    #[cfg(test)]
    pub fn verify_and_execute_block(
        &self,
        runner: &LocalRunner,
        sealed_block: SealedBlock,
    ) -> Result<UncommittedResult<Changes>, Error> {
        runner.run(move || {
            let result = Self::verify_and_execute_block_inner(
                &self.executor,
                &self.verifier,
                &sealed_block,
            );

            let VerifyAndExecutionResult {
                tx_status,
                events,
                changes,
            } = result?;

            let import_result =
                ImportResult::new_from_network(sealed_block, tx_status, events);

            Ok(UncommittedResult::new(import_result, changes))
        })
    }

    fn verify_and_execute_block_inner(
        executor: &E,
        verifier: &V,
        sealed_block: &SealedBlock,
    ) -> Result<VerifyAndExecutionResult, Error> {
        let consensus = &sealed_block.consensus;
        let block = &sealed_block.entity;
        let sealed_block_id = block.id();

        let result_of_verification = verifier.verify_block_fields(consensus, block);
        if let Err(err) = result_of_verification {
            return Err(Error::FailedVerification(err))
        }

        // The current code has a separate function X to process `StateConfig`.
        // It is not possible to execute it via `Executor`.
        // Maybe we need consider to move the function X here, if that possible.
        if let Consensus::Genesis(_) = consensus {
            return Err(Error::ExecuteGenesis)
        }

        let (ValidationResult { tx_status, events }, changes) = executor
            .validate(block)
            .map_err(Error::FailedExecution)?
            .into();

        let actual_block_id = block.id();
        if actual_block_id != sealed_block_id {
            // It should not be possible because, during validation, we don't touch the block.
            // But while we pass it by value, let's check it.
            return Err(Error::BlockIdMismatch(sealed_block_id, actual_block_id))
        }

        let result = VerifyAndExecutionResult {
            tx_status,
            events,
            changes,
        };

        Ok(result)
    }
}

impl Importer {
    /// The method validates the `Block` fields and commits the `SealedBlock`.
    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> Result<(), Error> {
        let _guard = self.lock()?;

        let start = Instant::now();
        let result = self.run_prepare_import_result(sealed_block).await?;
        let execute_time = start.elapsed().as_secs_f64();

        // Await until all receivers of the notification process the result.
        const TIMEOUT: u64 = 20;
        let await_result = tokio::time::timeout(
            Duration::from_secs(TIMEOUT),
            self.active_import_results.clone().acquire_owned(),
        )
        .await;

        let Ok(permit) = await_result else {
            tracing::error!(
                "The previous block processing \
                     was not finished for {TIMEOUT} seconds."
            );
            return Err(Error::PreviousBlockProcessingNotFinished)
        };
        let permit = permit.map_err(Error::ActiveBlockResultsSemaphoreClosed)?;

        let start = Instant::now();
        let commit_result = self
            .run_commit_result(permit, CommitInput::PrepareImportResult(result))
            .await;

        let commit_time = start.elapsed().as_secs_f64();
        let time = execute_time + commit_time;

        importer_metrics().execute_and_commit_duration.observe(time);

        commit_result
    }
}

/// The wrapper around `ImportResult` to notify about the end of the processing of a new block.
struct Awaiter {
    result: ImportResult,
    _permit: OwnedSemaphorePermit,
}

impl Deref for Awaiter {
    type Target = ImportResult;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

impl Awaiter {
    fn new(result: ImportResult, permit: OwnedSemaphorePermit) -> Self {
        Self {
            result,
            _permit: permit,
        }
    }
}

fn create_block_changes<D: ImporterDatabase + Transactional>(
    chain_id: &ChainId,
    sealed_block: &SealedBlock,
    database: &D,
) -> Result<Changes, Error> {
    let consensus = &sealed_block.consensus;
    let actual_next_height = *sealed_block.entity.header().height();

    // During importing of the genesis block, the database should not be initialized
    // and the genesis block defines the next height.
    // During the production of the non-genesis block, the next height should be underlying
    // database height + 1.
    let expected_next_height = match consensus {
        Consensus::Genesis(_) => {
            let result = database.latest_block_height()?;
            let found = result.is_some();
            // Because the genesis block is not committed, it should return `None`.
            // If we find the latest height, something is wrong with the state of the database.
            if found {
                return Err(Error::InvalidUnderlyingDatabaseGenesisState)
            }
            actual_next_height
        }
        Consensus::PoA(_) => {
            if actual_next_height == BlockHeight::from(0u32) {
                return Err(Error::ZeroNonGenericHeight)
            }

            let last_db_height = database
                .latest_block_height()?
                .ok_or(not_found!("Latest block height"))?;
            last_db_height
                .checked_add(1u32)
                .ok_or(Error::Overflow)?
                .into()
        }
        _ => {
            return Err(Error::UnsupportedConsensusVariant(format!(
                "{:?}",
                consensus
            )))
        }
    };

    if expected_next_height != actual_next_height {
        return Err(Error::IncorrectBlockHeight(
            expected_next_height,
            actual_next_height,
        ))
    }

    let mut transaction = database.storage_transaction(Changes::new());

    if !transaction.store_new_block(chain_id, sealed_block)? {
        return Err(Error::NotUnique(actual_next_height))
    }

    Ok(transaction.into_changes())
}
