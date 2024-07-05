use crate::{
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
    transactional::Changes,
    Error as StorageError,
    MerkleRoot,
};
use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Sealed,
        },
        primitives::BlockId,
        SealedBlock,
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
        executor,
        executor::ValidationResult,
        Uncommitted,
    },
};
use parking_lot::Mutex;
use std::{
    ops::{
        Deref,
        DerefMut,
    },
    sync::Arc,
    time::{
        Instant,
        SystemTime,
        UNIX_EPOCH,
    },
};
use tokio::sync::{
    broadcast,
    OwnedSemaphorePermit,
    Semaphore,
    TryAcquireError,
};

#[cfg(test)]
pub mod test;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "The commit is already in the progress: {_0}.")]
    SemaphoreError(TryAcquireError),
    #[display(
        fmt = "The wrong state of database during insertion of the genesis block."
    )]
    InvalidUnderlyingDatabaseGenesisState,
    #[display(fmt = "The wrong state of storage after execution of the block.\
        The actual root is {_1:?}, when the expected root is {_0:?}.")]
    InvalidDatabaseStateAfterExecution(Option<MerkleRoot>, Option<MerkleRoot>),
    #[display(fmt = "Got overflow during increasing the height.")]
    Overflow,
    #[display(fmt = "The non-generic block can't have zero height.")]
    ZeroNonGenericHeight,
    #[display(fmt = "The actual height is {_1}, when the next expected height is {_0}.")]
    IncorrectBlockHeight(BlockHeight, BlockHeight),
    #[display(
        fmt = "Got another block id after validation of the block. Expected {_0} != Actual {_1}"
    )]
    BlockIdMismatch(BlockId, BlockId),
    #[display(fmt = "Some of the block fields are not valid: {_0}.")]
    FailedVerification(anyhow::Error),
    #[display(fmt = "The execution of the block failed: {_0}.")]
    FailedExecution(executor::Error),
    #[display(fmt = "It is not possible to execute the genesis block.")]
    ExecuteGenesis,
    #[display(fmt = "The database already contains the data at the height {_0}.")]
    NotUnique(BlockHeight),
    #[display(fmt = "The previous block processing is not finished yet.")]
    PreviousBlockProcessingNotFinished,
    #[from]
    StorageError(StorageError),
    UnsupportedConsensusVariant(String),
    ActiveBlockResultsSemaphoreClosed(tokio::sync::AcquireError),
    RayonTaskWasCanceled,
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

#[cfg(test)]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        format!("{self}") == format!("{other}")
    }
}

pub struct Importer<D, E, V> {
    database: Mutex<D>,
    executor: Arc<E>,
    verifier: Arc<V>,
    chain_id: ChainId,
    broadcast: broadcast::Sender<ImporterResult>,
    guard: Semaphore,
    /// The semaphore tracks the number of unprocessed `SharedImportResult`.
    /// If the number of unprocessed results is more than the threshold,
    /// the block importer stops committing new blocks and waits for
    /// the resolution of the previous one.
    active_import_results: Arc<Semaphore>,
    process_thread: rayon::ThreadPool,
}

impl<D, E, V> Importer<D, E, V> {
    pub fn new(
        chain_id: ChainId,
        config: Config,
        database: D,
        executor: E,
        verifier: V,
    ) -> Self {
        // We use semaphore as a back pressure mechanism instead of a `broadcast`
        // channel because we want to prevent committing to the database results
        // that will not be processed.
        let max_block_notify_buffer = config.max_block_notify_buffer;
        let (broadcast, _) = broadcast::channel(max_block_notify_buffer);
        let process_thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("Failed to create a thread pool for the block processing");

        Self {
            database: Mutex::new(database),
            executor: Arc::new(executor),
            verifier: Arc::new(verifier),
            chain_id,
            broadcast,
            active_import_results: Arc::new(Semaphore::new(max_block_notify_buffer)),
            guard: Semaphore::new(1),
            process_thread,
        }
    }

    #[cfg(test)]
    pub fn default_config(database: D, executor: E, verifier: V) -> Self {
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
                Err(Error::SemaphoreError(err))
            }
        }
    }

    async fn async_run<OP, Output>(&self, op: OP) -> Result<Output, Error>
    where
        OP: FnOnce() -> Output,
        OP: Send,
        Output: Send,
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.process_thread.scope_fifo(|_| {
            let result = op();
            let _ = sender.send(result);
        });
        let result = receiver.await.map_err(|_| Error::RayonTaskWasCanceled)?;
        Ok(result)
    }
}

impl<D, E, V> Importer<D, E, V>
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
            tokio::time::Duration::from_secs(TIMEOUT),
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

        self.async_run(move || {
            let mut guard = self
                .database
                .try_lock()
                .expect("Semaphore prevents concurrent access to the database");
            let database = guard.deref_mut();
            self._commit_result(result, permit, database)
        })
        .await?
    }

    /// The method commits the result of the block execution and notifies about a new imported block.
    #[tracing::instrument(
        skip_all,
        fields(
            block_id = %result.result().sealed_block.entity.id(),
            height = **result.result().sealed_block.entity.header().height(),
            tx_status = ?result.result().tx_status,
        ),
        err
    )]
    fn _commit_result(
        &self,
        result: UncommittedResult<Changes>,
        permit: OwnedSemaphorePermit,
        database: &mut D,
    ) -> Result<(), Error> {
        let (result, changes) = result.into();
        let block = &result.sealed_block.entity;
        let consensus = &result.sealed_block.consensus;
        let actual_next_height = *block.header().height();

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

        // Importer expects that `UncommittedResult` contains the result of block
        // execution without block itself.
        let expected_block_root = database.latest_block_root()?;

        #[cfg(feature = "test-helpers")]
        let changes_clone = changes.clone();
        let mut db_after_execution = database.storage_transaction(changes);
        let actual_block_root = db_after_execution.latest_block_root()?;
        if actual_block_root != expected_block_root {
            return Err(Error::InvalidDatabaseStateAfterExecution(
                expected_block_root,
                actual_block_root,
            ))
        }

        if !db_after_execution.store_new_block(&self.chain_id, &result.sealed_block)? {
            return Err(Error::NotUnique(expected_next_height))
        }

        db_after_execution.commit()?;

        // update the importer metrics after the block is successfully committed
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
            .try_lock()
            .expect("Init function is the first to access the database")
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
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    E: Validator,
    V: BlockVerifier,
{
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
    pub fn verify_and_execute_block(
        &self,
        sealed_block: SealedBlock,
    ) -> Result<UncommittedResult<Changes>, Error> {
        Self::verify_and_execute_block_inner(
            self.executor.clone(),
            self.verifier.clone(),
            sealed_block,
        )
    }

    fn verify_and_execute_block_inner(
        executor: Arc<E>,
        verifier: Arc<V>,
        sealed_block: SealedBlock,
    ) -> Result<UncommittedResult<Changes>, Error> {
        let consensus = sealed_block.consensus;
        let block = sealed_block.entity;
        let sealed_block_id = block.id();

        let result_of_verification = verifier.verify_block_fields(&consensus, &block);
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
            .validate(&block)
            .map_err(Error::FailedExecution)?
            .into();

        let actual_block_id = block.id();
        if actual_block_id != sealed_block_id {
            // It should not be possible because, during validation, we don't touch the block.
            // But while we pass it by value, let's check it.
            return Err(Error::BlockIdMismatch(sealed_block_id, actual_block_id))
        }

        let sealed_block = Sealed {
            entity: block,
            consensus,
        };
        let import_result =
            ImportResult::new_from_network(sealed_block, tx_status, events);

        Ok(Uncommitted::new(import_result, changes))
    }
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    IDatabase: ImporterDatabase + Transactional + 'static,
    E: Validator + 'static,
    V: BlockVerifier + 'static,
{
    /// The method validates the `Block` fields and commits the `SealedBlock`.
    /// It is a combination of the [`Importer::verify_and_execute_block`] and [`Importer::commit_result`].
    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> Result<(), Error> {
        let _guard = self.lock()?;

        let executor = self.executor.clone();
        let verifier = self.verifier.clone();
        let (result, execute_time) = self
            .async_run(|| {
                let start = Instant::now();
                let result = Self::verify_and_execute_block_inner(
                    executor,
                    verifier,
                    sealed_block,
                );
                let execute_time = start.elapsed().as_secs_f64();
                (result, execute_time)
            })
            .await?;

        let result = result?;

        // Await until all receivers of the notification process the result.
        const TIMEOUT: u64 = 20;
        let await_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(TIMEOUT),
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

        let commit_result = self
            .async_run(move || {
                let mut guard = self
                    .database
                    .try_lock()
                    .expect("Semaphore prevents concurrent access to the database");
                let database = guard.deref_mut();

                let start = Instant::now();
                self._commit_result(result, permit, database).map(|_| start)
            })
            .await?;

        let time = if let Ok(start_instant) = commit_result {
            let commit_time = start_instant.elapsed().as_secs_f64();
            execute_time + commit_time
        } else {
            execute_time
        };

        importer_metrics().execute_and_commit_duration.observe(time);
        // return execution result
        commit_result.map(|_| ())
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
