use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    tables::{
        merkle::{
            DenseMetadataKey,
            FuelBlockMerkleMetadata,
        },
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::{
        Changes,
        ConflictPolicy,
        Modifiable,
        StorageTransaction,
        WriteTransaction,
    },
    MerkleRoot,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            BlockV1,
        },
        consensus::Consensus,
        header::ConsensusParametersVersion,
        SealedBlock,
    },
    fuel_tx::TxId,
    fuel_types::{
        BlockHeight,
        ChainId,
    },
    services::executor::{
        Result as ExecutorResult,
        UncommittedValidationResult,
    },
};

#[cfg_attr(test, mockall::automock(type Database = crate::importer::test::MockDatabase;))]
/// The executors port.
pub trait Validator: Send + Sync {
    /// Executes the block and returns the result of execution with uncommitted database
    /// transaction.
    fn validate(
        &self,
        block: &Block,
    ) -> ExecutorResult<UncommittedValidationResult<Changes>>;
}

/// The trait indicates that the type supports storage transactions.
pub trait Transactional {
    /// The type of the storage transaction;
    type Transaction<'a>: DatabaseTransaction
    where
        Self: 'a;

    /// Returns the storage transaction based on the `Changes`.
    fn storage_transaction(&mut self, changes: Changes) -> Self::Transaction<'_>;
}

/// The alias port used by the block importer.
pub trait ImporterDatabase: Send + Sync {
    /// Returns the latest block height.
    fn latest_block_height(&self) -> StorageResult<Option<BlockHeight>>;

    /// Returns the latest block root.
    fn latest_block_root(&self) -> StorageResult<Option<MerkleRoot>>;

    /// Returns chain id.
    fn chain_id(
        &self,
        consensus_parameters_version: &ConsensusParametersVersion,
    ) -> StorageResult<Option<ChainId>>;
}

/// The port of the storage transaction required by the importer.
#[cfg_attr(test, mockall::automock)]
pub trait DatabaseTransaction {
    /// Returns the latest block root.
    fn latest_block_root(&self) -> StorageResult<Option<MerkleRoot>>;

    /// Inserts the `SealedBlock`.
    ///
    /// The method returns `true` if the block is a new, otherwise `false`.
    // TODO: Remove `chain_id` from the signature, but for that transactions inside
    //  the block should have `cached_id`. We need to guarantee that from the Rust-type system.
    fn store_new_block(
        &mut self,
        block: &SealedBlock,
        tx_ids: Vec<TxId>,
    ) -> StorageResult<()>;

    /// Commits the changes to the underlying storage.
    fn commit(self) -> StorageResult<()>;
}

#[cfg_attr(test, mockall::automock)]
/// The verifier of the block.
pub trait BlockVerifier: Send + Sync {
    /// Verifies the consistency of the block fields for the block's height.
    /// It includes the verification of **all** fields, it includes the consensus rules for
    /// the corresponding height.
    ///
    /// Return an error if the verification failed, otherwise `Ok(())`.
    fn verify_block_fields(
        &self,
        consensus: &Consensus,
        block: &Block,
        verify_transactions_root: bool,
    ) -> anyhow::Result<()>;
}

impl<S> Transactional for S
where
    S: KeyValueInspect<Column = Column> + Modifiable,
{
    type Transaction<'a> = StorageTransaction<&'a mut S> where Self: 'a;

    fn storage_transaction(&mut self, changes: Changes) -> Self::Transaction<'_> {
        self.write_transaction()
            .with_changes(changes)
            .with_policy(ConflictPolicy::Fail)
    }
}

impl<S> DatabaseTransaction for StorageTransaction<S>
where
    S: KeyValueInspect<Column = Column> + Modifiable,
{
    fn latest_block_root(&self) -> StorageResult<Option<MerkleRoot>> {
        Ok(self
            .storage_as_ref::<FuelBlockMerkleMetadata>()
            .get(&DenseMetadataKey::Latest)?
            .map(|cow| *cow.root()))
    }

    fn store_new_block(
        &mut self,
        block: &SealedBlock,
        tx_ids: Vec<TxId>,
    ) -> StorageResult<()> {
        // Replace all with insert
        let height = block.entity.header().height();

        // TODO: Use `batch_insert` from https://github.com/FuelLabs/fuel-core/pull/1576
        let start = tokio::time::Instant::now();
        for (tx, tx_id) in block.entity.transactions().iter().zip(tx_ids.iter()) {
            // Maybe a debug insert
            self.storage_as_mut::<Transactions>().insert(tx_id, tx)?;
        }
        tracing::info!(
            "Insert transactions in store_new_block took {} milliseconds",
            start.elapsed().as_millis()
        );

        // Compress is really doing recomputation of id ? it shouldn't.
        // Should be fast
        let start = tokio::time::Instant::now();
        let compressed_block = {
            let new_inner = BlockV1 {
                header: block.entity.header().clone(),
                transactions: tx_ids,
            };
            Block::V1(new_inner)
        };
        tracing::info!(
            "Compress in store_new_block took {} milliseconds",
            start.elapsed().as_millis()
        );
        let start = tokio::time::Instant::now();
        self.storage_as_mut::<FuelBlocks>()
            .insert(height, &compressed_block)?;
        tracing::info!(
            "Insert block in store_new_block took {} milliseconds (including compress)",
            start.elapsed().as_millis()
        );
        let start = tokio::time::Instant::now();
        self.storage_as_mut::<SealedBlockConsensus>()
            .insert(height, &block.consensus)?;
        tracing::info!(
            "Insert consensus in store_new_block took {} milliseconds",
            start.elapsed().as_millis()
        );

        Ok(())
    }

    fn commit(self) -> StorageResult<()> {
        self.commit()?;
        Ok(())
    }
}
