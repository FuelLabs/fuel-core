use crate::ports::{
    Database,
    Executor,
};
use anyhow::anyhow;
use fuel_core_storage::transactional::StorageTransaction;
use fuel_core_types::{
    blockchain::{
        consensus::Sealed,
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
use tokio::sync::broadcast;

pub struct Importer<E, D> {
    executor: E,
    database: D,
    broadcast: broadcast::Sender<ImportResult>,
    guard: tokio::sync::Mutex<()>,
}

impl<E, D> Importer<E, D> {
    pub fn new(executor: E, database: D) -> Self {
        let (broadcast, _) = broadcast::channel(usize::MAX >> 1);
        Self {
            executor,
            database,
            broadcast,
            guard: Default::default(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ImportResult> {
        self.broadcast.subscribe()
    }
}

impl<E, D> Importer<E, D>
where
    D: Database,
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
    /// Only one commit may be in progress at the time. All other calls will be suspended until
    /// the first is done. It uses the `tokio::sync::Mutex`, so the parent task will wake up
    /// when it is ready.
    pub async fn commit_result(
        &self,
        result: UncommittedResult<StorageTransaction<D>>,
    ) -> anyhow::Result<()> {
        let _guard = self.guard.lock().await;

        let (result, mut db_tx) = result.into();
        let block = &result.block.entity;
        let block_id = block.id();
        let height = block.header().height();

        // The state of the underlying database should be at the `height` - 1.
        if self.database.latest_block_height()?.as_usize() + 1 != height.as_usize() {
            return Err(anyhow!(
                "The database not on the height {}",
                height.as_usize() - 1
            ))
        }

        let db = db_tx.as_mut();

        // Importer expects that `UncommittedResult` contains the result of block
        // execution(It includes the block itself).
        if db.latest_block_height()? != *height {
            return Err(anyhow!(
                "`UncommittedResult` doesn't have the block at height {}",
                height
            ))
        }

        db.seal_block(&block_id, &result.block.consensus)?
            .should_be_unique(height)?;

        // TODO: This should use a proper BMT MMR. Based on peaks stored in the database,
        //  we need to calculate a new root. The data type that will do that should live
        //  in the `fuel-core-storage` or `fuel-merkle` crate.
        let root = ephemeral_merkle_root(
            vec![*block.header().prev_root(), block_id.into()].iter(),
        );
        db.insert_block_header_merkle_root(height, &root)?
            .should_be_unique(height)?;

        db_tx.commit()?;

        let _ = self.broadcast.send(result);
        Ok(())
    }
}

impl<E, D> Importer<E, D>
where
    D: Database,
    E: Executor<D>,
{
    /// The method validates the `Block` fields, executes the `Block`, and commits it with data from
    /// `SealedBlock`. More about the commit process in [`Importer::commit_result`].
    ///
    /// It validates only the `Block` execution rules and the `Block` fields' validity.
    /// The validity of the `SealedBlock` and seal information is not the concern of this function.
    pub async fn execute_and_commit(
        &self,
        sealed_block: SealedBlock,
    ) -> anyhow::Result<()> {
        // TODO: Validate the block `height`
        // TODO: Validate the block `time`
        // TODO: Validate the block `prev_root`
        // TODO: Validate the block `da_height`

        let consensus = sealed_block.consensus;
        let block = sealed_block.entity;
        let sealed_block_id = block.id();

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
            block: Sealed {
                entity: block,
                consensus,
            },
            tx_status,
        };

        self.commit_result(Uncommitted::new(import_result, db_tx))
            .await
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
