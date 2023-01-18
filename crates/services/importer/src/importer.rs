use crate::{
    ports::{
        BlockVerifier,
        Executor,
        ExecutorDatabase,
        ImporterDatabase,
    },
    Config,
};
use anyhow::anyhow;
use fuel_core_storage::{
    transactional::StorageTransaction,
    IsNotFound,
};
use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Sealed,
        },
        primitives::BlockHeight,
        SealedBlock,
    },
    fuel_vm::crypto::ephemeral_merkle_root,
    services::{
        block_importer::{
            ImportResult,
            UncommittedResult,
        },
        executor::{
            ExecutionBlock,
            ExecutionResult,
        },
        Uncommitted,
    },
};
use std::sync::Arc;
use tokio::sync::broadcast;

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

    fn lock(&self) -> anyhow::Result<tokio::sync::SemaphorePermit> {
        let guard = self.guard.try_acquire();
        match guard {
            Ok(permit) => Ok(permit),
            Err(err) => {
                tracing::error!(
                    "The semaphore was acquired before. It is a problem \
                    because the current architecture doesn't expect that."
                );
                return Err(anyhow!("The commit is already in the progress: {}", err))
            }
        }
    }
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    IDatabase: ImporterDatabase,
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
    /// Only one commit may be in progress at the time. All other calls will be fail.
    pub fn commit_result<EDatabase>(
        &self,
        result: UncommittedResult<StorageTransaction<EDatabase>>,
    ) -> anyhow::Result<()>
    where
        EDatabase: ExecutorDatabase,
    {
        let _guard = self.lock()?;
        self._commit_result(result)
    }

    fn _commit_result<EDatabase>(
        &self,
        result: UncommittedResult<StorageTransaction<EDatabase>>,
    ) -> anyhow::Result<()>
    where
        EDatabase: ExecutorDatabase,
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
                    return Err(anyhow!("The wrong state of database during insertion of the genesis block"))
                }
                actual_next_height
            }
            Consensus::PoA(_) => {
                let last_db_height = self.database.latest_block_height()?;
                last_db_height
                    .checked_add(1u32)
                    .ok_or_else(|| {
                        anyhow!("An overflow during the calculation of the next height")
                    })?
                    .into()
            }
        };

        if expected_next_height != actual_next_height {
            return Err(anyhow!(
                "The actual height is {}, when the next expected height is {}",
                actual_next_height.as_usize(),
                expected_next_height.as_usize(),
            ))
        }

        let db_after_execution = db_tx.as_mut();

        // Importer expects that `UncommittedResult` contains the result of block
        // execution(It includes the block itself).
        if db_after_execution.latest_block_height()? != actual_next_height {
            return Err(anyhow!(
                "`UncommittedResult` doesn't have the block at height {}",
                actual_next_height
            ))
        }

        db_after_execution
            .seal_block(&block_id, &result.sealed_block.consensus)?
            .should_be_unique(&actual_next_height)?;

        // TODO: This should use a proper BMT MMR. Based on peaks stored in the database,
        //  we need to calculate a new root. The data type that will do that should live
        //  in the `fuel-core-storage` or `fuel-merkle` crate.
        let root = ephemeral_merkle_root(
            vec![*block.header().prev_root(), block_id.into()].iter(),
        );
        db_after_execution
            .insert_block_header_merkle_root(&actual_next_height, &root)?
            .should_be_unique(&actual_next_height)?;

        db_tx.commit()?;

        let _ = self.broadcast.send(Arc::new(result));
        Ok(())
    }
}

impl<IDatabase, E, V> Importer<IDatabase, E, V>
where
    IDatabase: ImporterDatabase,
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
    pub fn verify_block(
        &self,
        sealed_block: SealedBlock,
    ) -> anyhow::Result<UncommittedResult<StorageTransaction<E::Database>>> {
        let consensus = sealed_block.consensus;
        let block = sealed_block.entity;
        let sealed_block_id = block.id();

        let result_of_verification =
            self.verifier.verify_block_fields(&consensus, &block);
        if let Err(err) = result_of_verification {
            return Err(anyhow!("Some of the block fields are not valid: {}", err))
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
            .execute_without_commit(ExecutionBlock::Validation(block))?
            .into();

        // If we skipped transaction, it means that the block is invalid.
        if !skipped_transactions.is_empty() {
            return Err(anyhow!(
                "It is not possible to skip transactions during importing of the block"
            ))
        }

        if block.id() != sealed_block_id {
            // It should not be possible because, during validation, we don't touch the block.
            // But while we pass it by value, let's check it.
            return Err(anyhow!("Got another id after validation of the block"))
        }

        let import_result = ImportResult {
            sealed_block: Sealed {
                entity: block,
                consensus,
            },
            tx_status,
        };

        Ok(Uncommitted::new(import_result, db_tx))
    }

    /// The method validates the `Block` fields and commits the `SealedBlock`.
    /// It is a combination of the [`Importer::verify_block`] and [`Importer::commit_result`].
    pub fn execute_and_commit(&self, sealed_block: SealedBlock) -> anyhow::Result<()> {
        let _guard = self.lock()?;
        let result = self.verify_block(sealed_block)?;
        self._commit_result(result)
    }
}

trait ShouldBeUnique {
    fn should_be_unique(&self, height: &BlockHeight) -> anyhow::Result<()>;
}

impl<T> ShouldBeUnique for Option<T> {
    fn should_be_unique(&self, height: &BlockHeight) -> anyhow::Result<()> {
        if self.is_some() {
            return Err(anyhow!(
                "The database already contains the data at the height {}",
                height,
            ))
        } else {
            Ok(())
        }
    }
}
