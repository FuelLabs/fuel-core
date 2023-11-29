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
    transactional::StorageTransaction,
    Error as StorageError,
    IsNotFound,
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
    fuel_types::BlockHeight,
    services::{
        block_importer::{
            ImportResult,
            UncommittedResult,
        },
        executor,
        executor::ExecutionResult,
        Uncommitted,
    },
};
use std::{
    ops::Deref,
    sync::Arc,
    time::{
        Instant,
        SystemTime,
        UNIX_EPOCH,
    },
};
use tokio::sync::{
    broadcast,
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
        The actual height is {_1}, when the next expected height is {_0}.")]
    InvalidDatabaseStateAfterExecution(BlockHeight, BlockHeight),
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
}

impl From<Error> for anyhow::Error {
    fn from(error: Error) -> Self {
        anyhow::Error::msg(error)
    }
}

#[cfg(test)]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

pub struct Importer<D, E, V> {
    database: D,
    executor: E,
    verifier: V,
    broadcast: broadcast::Sender<Arc<ImportResult>>,
    guard: tokio::sync::Semaphore,
}

impl<D, E, V> Importer<D, E, V> {
    pub fn new(config: Config, database: D, executor: E, verifier: V) -> Self {
        let (broadcast, _) = broadcast::channel(config.max_block_notify_buffer);

        Self {
            database,
            executor,
            verifier,
            broadcast,
            guard: tokio::sync::Semaphore::new(1),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<ImportResult>> {
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
    pub fn commit_result<ExecutorDatabase>(
        &self,
        result: UncommittedResult<StorageTransaction<ExecutorDatabase>>,
    ) -> Result<(), Error>
    where
        ExecutorDatabase: ports::ExecutorDatabase,
    {
        let _guard = self.lock()?;
        self._commit_result(result)
    }

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
        let block_id = block.id();
        let actual_next_height = *block.header().height();

        // During importing of the genesis block, the database should not be initialized
        // and the genesis block defines the next height.
        // During the production of the non-genesis block, the next height should be underlying
        // database height + 1.
        let expected_next_height = match consensus {
            Consensus::Genesis(_) => {
                let result = self.database.latest_block_height();
                let found = !result.is_not_found();
                // Because the genesis block is not committed, it should return non found error.
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

                let last_db_height = self.database.latest_block_height()?;
                last_db_height
                    .checked_add(1u32)
                    .ok_or(Error::Overflow)?
                    .into()
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
        // execution(It includes the block itself).
        let actual_height = db_after_execution.latest_block_height()?;
        if expected_next_height != actual_height {
            return Err(Error::InvalidDatabaseStateAfterExecution(
                expected_next_height,
                actual_height,
            ))
        }

        db_after_execution
            .seal_block(&block_id, &result.sealed_block.consensus)?
            .should_be_unique(&expected_next_height)?;

        // Update the total tx count in chain metadata
        let total_txs = db_after_execution
            // Safety: casting len to u64 since it's impossible to execute a block with more than 2^64 txs
            .increase_tx_count(result.sealed_block.entity.transactions().len() as u64)?;

        db_tx.commit()?;

        // update the importer metrics after the block is successfully committed
        importer_metrics().total_txs_count.set(total_txs as i64);
        importer_metrics()
            .block_height
            .set(*actual_height.deref() as i64);
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        importer_metrics()
            .latest_block_import_timestamp
            .set(current_time);

        tracing::info!("Committed block {:#x}", result.sealed_block.entity.id());
        let _ = self.broadcast.send(Arc::new(result));
        Ok(())
    }

    /// Should only be called once after startup to set importer metrics to their initial values
    pub fn init_metrics(&self) {
        // load starting values from database

        // Errors are optimistically handled via fallback to default values since the metrics
        // should get updated regularly anyways and these errors will be discovered and handled
        // correctly in more mission critical areas (such as _commit_result)
        let current_block_height =
            self.database.latest_block_height().unwrap_or_default();
        let total_tx_count = self.database.increase_tx_count(0).unwrap_or_default();

        importer_metrics()
            .total_txs_count
            .set(total_tx_count as i64);
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
        let consensus = sealed_block.consensus;
        let block = sealed_block.entity;
        let sealed_block_id = block.id();

        let result_of_verification =
            self.verifier.verify_block_fields(&consensus, &block);
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
            },
            db_tx,
        ) = self
            .executor
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
        let import_result = ImportResult::new_from_network(sealed_block, tx_status);

        Ok(Uncommitted::new(import_result, db_tx))
    }
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    IDatabase: ImporterDatabase,
    E: Executor,
    V: BlockVerifier,
{
    /// The method validates the `Block` fields and commits the `SealedBlock`.
    /// It is a combination of the [`Importer::verify_and_execute_block`] and [`Importer::commit_result`].
    pub fn execute_and_commit(&self, sealed_block: SealedBlock) -> Result<(), Error> {
        let _guard = self.lock()?;
        let start = Instant::now();
        let result = self.verify_and_execute_block(sealed_block)?;
        let commit_result = self._commit_result(result);
        // record the execution time to prometheus
        let time = start.elapsed().as_secs_f64();
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
