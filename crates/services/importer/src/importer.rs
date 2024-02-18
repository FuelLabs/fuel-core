use crate::{
    ports,
    ports::{
        BlockVerifier,
        Executor,
        ImporterDatabase,
    },
    Config,
};
use fuel_core_metrics::importer::importer_metrics;
use fuel_core_storage::{
    not_found,
    transactional::StorageTransaction,
    Error as StorageError,
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
            SharedImportResult,
            UncommittedResult,
        },
        executor,
        executor::ExecutionResult,
        Uncommitted,
    },
};
use std::{
    ops::Deref,
    sync::{
        Arc,
        Mutex,
    },
    time::{
        Instant,
        SystemTime,
        UNIX_EPOCH,
    },
};
use tokio::sync::{
    broadcast,
    oneshot,
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
    #[display(fmt = "The wrong state of database after execution of the block.\
        The actual height is {_1:?}, when the next expected height is {_0:?}.")]
    InvalidDatabaseStateAfterExecution(Option<BlockHeight>, Option<BlockHeight>),
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
    #[display(
        fmt = "It is not possible to skip transactions during importing of the block."
    )]
    SkippedTransactionsNotEmpty,
    #[display(fmt = "It is not possible to execute the genesis block.")]
    ExecuteGenesis,
    #[display(fmt = "The database already contains the data at the height {_0}.")]
    NotUnique(BlockHeight),
    #[from]
    StorageError(StorageError),
    UnsupportedConsensusVariant(String),
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
    database: D,
    executor: Arc<E>,
    verifier: Arc<V>,
    chain_id: ChainId,
    broadcast: broadcast::Sender<SharedImportResult>,
    /// The channel to notify about the end of the processing of the previous block by all listeners.
    /// It is used to await until all receivers of the notification process the `SharedImportResult`
    /// before starting committing a new block.
    prev_block_process_result: Mutex<Option<oneshot::Receiver<()>>>,
    guard: tokio::sync::Semaphore,
}

impl<D, E, V> Importer<D, E, V> {
    pub fn new(config: Config, database: D, executor: E, verifier: V) -> Self {
        let (broadcast, _) = broadcast::channel(config.max_block_notify_buffer);

        Self {
            database,
            executor: Arc::new(executor),
            verifier: Arc::new(verifier),
            chain_id: config.chain_id,
            broadcast,
            prev_block_process_result: Default::default(),
            guard: tokio::sync::Semaphore::new(1),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SharedImportResult> {
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
}

impl<D, E, V> Importer<D, E, V>
where
    D: ImporterDatabase,
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
    pub async fn commit_result<ExecutorDatabase>(
        &self,
        result: UncommittedResult<StorageTransaction<ExecutorDatabase>>,
    ) -> Result<(), Error>
    where
        ExecutorDatabase: ports::ExecutorDatabase,
    {
        let _guard = self.lock()?;
        // It is safe to unwrap the channel because we have the `_guard`.
        let previous_block_result = self
            .prev_block_process_result
            .lock()
            .expect("poisoned")
            .take();

        // Await until all receivers of the notification process the result.
        if let Some(channel) = previous_block_result {
            let _ = channel.await;
        }

        self._commit_result(result)
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
    fn _commit_result<ExecutorDatabase>(
        &self,
        result: UncommittedResult<StorageTransaction<ExecutorDatabase>>,
    ) -> Result<(), Error>
    where
        ExecutorDatabase: ports::ExecutorDatabase,
    {
        let (result, mut db_tx) = result.into();
        let block = &result.sealed_block.entity;
        let consensus = &result.sealed_block.consensus;
        let actual_next_height = *block.header().height();

        // During importing of the genesis block, the database should not be initialized
        // and the genesis block defines the next height.
        // During the production of the non-genesis block, the next height should be underlying
        // database height + 1.
        let expected_next_height = match consensus {
            Consensus::Genesis(_) => {
                let result = self.database.latest_block_height()?;
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

                let last_db_height = self
                    .database
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

        let db_after_execution = db_tx.as_mut();

        // Importer expects that `UncommittedResult` contains the result of block
        // execution without block itself.
        let expected_height = self.database.latest_block_height()?;
        let actual_height = db_after_execution.latest_block_height()?;
        if expected_height != actual_height {
            return Err(Error::InvalidDatabaseStateAfterExecution(
                expected_height,
                actual_height,
            ))
        }

        if !db_after_execution.store_new_block(&self.chain_id, &result.sealed_block)? {
            return Err(Error::NotUnique(expected_next_height))
        }

        db_tx.commit()?;

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

        // The `tokio::sync::oneshot::Sender` is used to notify about the end
        // of the processing of a new block by all listeners.
        let (sender, receiver) = oneshot::channel();
        let _ = self.broadcast.send(Arc::new(Awaiter::new(result, sender)));
        *self.prev_block_process_result.lock().expect("poisoned") = Some(receiver);

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
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    E: Executor,
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
    ) -> Result<UncommittedResult<StorageTransaction<E::Database>>, Error> {
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
    ) -> Result<UncommittedResult<StorageTransaction<E::Database>>, Error> {
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

        // TODO: Pass `block` into `ExecutionBlock::Validation` by ref
        let (
            ExecutionResult {
                block,
                skipped_transactions,
                tx_status,
                events,
            },
            db_tx,
        ) = executor
            .execute_without_commit(block)
            .map_err(Error::FailedExecution)?
            .into();

        // If we skipped transaction, it means that the block is invalid.
        if !skipped_transactions.is_empty() {
            return Err(Error::SkippedTransactionsNotEmpty)
        }

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

        Ok(Uncommitted::new(import_result, db_tx))
    }
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    IDatabase: ImporterDatabase + 'static,
    E: Executor + 'static,
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
        let (result, execute_time) = tokio_rayon::spawn_fifo(|| {
            let start = Instant::now();
            let result =
                Self::verify_and_execute_block_inner(executor, verifier, sealed_block);
            let execute_time = start.elapsed().as_secs_f64();
            (result, execute_time)
        })
        .await;

        let result = result?;

        // It is safe to unwrap the channel because we have the `_guard`.
        let previous_block_result = self
            .prev_block_process_result
            .lock()
            .expect("poisoned")
            .take();

        // Await until all receivers of the notification process the result.
        if let Some(channel) = previous_block_result {
            let _ = channel.await;
        }

        let start = Instant::now();
        let commit_result = self._commit_result(result);
        let commit_time = start.elapsed().as_secs_f64();
        let time = execute_time + commit_time;
        importer_metrics().execute_and_commit_duration.observe(time);
        // return execution result
        commit_result
    }
}

trait ShouldBeUnique {
    fn should_be_unique(&self, height: &BlockHeight) -> Result<(), Error>;
}

impl<T> ShouldBeUnique for Option<T> {
    fn should_be_unique(&self, height: &BlockHeight) -> Result<(), Error> {
        if self.is_some() {
            Err(Error::NotUnique(*height))
        } else {
            Ok(())
        }
    }
}

/// The wrapper around `ImportResult` to notify about the end of the processing of a new block.
struct Awaiter {
    result: ImportResult,
    release_channel: Option<oneshot::Sender<()>>,
}

impl Drop for Awaiter {
    fn drop(&mut self) {
        if let Some(release_channel) = core::mem::take(&mut self.release_channel) {
            let _ = release_channel.send(());
        }
    }
}

impl Deref for Awaiter {
    type Target = ImportResult;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

impl Awaiter {
    fn new(result: ImportResult, channel: oneshot::Sender<()>) -> Self {
        Self {
            result,
            release_channel: Some(channel),
        }
    }
}
