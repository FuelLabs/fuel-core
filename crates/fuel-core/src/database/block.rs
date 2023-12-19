use crate::database::{
    storage::{
        DenseMerkleMetadata,
        FuelBlockMerkleData,
        FuelBlockMerkleMetadata,
        FuelBlockSecondaryKeyBlockHeights,
        ToDatabaseKey,
    },
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
};
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
    tables::{
        FuelBlocks,
        Transactions,
    },
    Error as StorageError,
    MerkleRootStorage,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            CompressedBlock,
        },
        primitives::BlockId,
    },
    entities::message::MerkleProof,
    fuel_merkle::binary::MerkleTree,
    fuel_types::BlockHeight,
    tai64::Tai64,
};
use itertools::Itertools;
use std::{
    borrow::{
        BorrowMut,
        Cow,
    },
    convert::{
        TryFrom,
        TryInto,
    },
};

impl StorageInspect<FuelBlocks> for Database {
    type Error = StorageError;

    fn get(&self, key: &BlockId) -> Result<Option<Cow<CompressedBlock>>, Self::Error> {
        Database::get(self, key.as_slice(), Column::FuelBlocks).map_err(Into::into)
    }

    fn contains_key(&self, key: &BlockId) -> Result<bool, Self::Error> {
        Database::contains_key(self, key.as_slice(), Column::FuelBlocks)
            .map_err(Into::into)
    }
}

impl StorageMutate<FuelBlocks> for Database {
    fn insert(
        &mut self,
        key: &BlockId,
        value: &CompressedBlock,
    ) -> Result<Option<CompressedBlock>, Self::Error> {
        let prev = Database::insert(self, key.as_slice(), Column::FuelBlocks, value)?;

        let height = value.header().height();
        self.storage::<FuelBlockSecondaryKeyBlockHeights>()
            .insert(height, key)?;

        // Get latest metadata entry
        let prev_metadata = self
            .iter_all::<Vec<u8>, DenseMerkleMetadata>(
                Column::FuelBlockMerkleMetadata,
                Some(IterDirection::Reverse),
            )
            .next()
            .transpose()?
            .map(|(_, metadata)| metadata)
            .unwrap_or_default();

        let storage = self.borrow_mut();
        let mut tree: MerkleTree<FuelBlockMerkleData, _> =
            MerkleTree::load(storage, prev_metadata.version)
                .map_err(|err| StorageError::Other(anyhow::anyhow!(err)))?;
        let data = key.as_slice();
        tree.push(data)?;

        // Generate new metadata for the updated tree
        let version = tree.leaves_count();
        let root = tree.root();
        let metadata = DenseMerkleMetadata { version, root };
        self.storage::<FuelBlockMerkleMetadata>()
            .insert(height, &metadata)?;

        Ok(prev)
    }

    fn remove(&mut self, key: &BlockId) -> Result<Option<CompressedBlock>, Self::Error> {
        let prev: Option<CompressedBlock> =
            Database::take(self, key.as_slice(), Column::FuelBlocks)?;

        if let Some(block) = &prev {
            let height = block.header().height();
            let _ = self
                .storage::<FuelBlockSecondaryKeyBlockHeights>()
                .remove(height);
            // We can't clean up `MerkleTree<FuelBlockMerkleData>`.
            // But if we plan to insert a new block, it will override old values in the
            // `FuelBlockMerkleData` table.
            let _ = self.storage::<FuelBlockMerkleMetadata>().remove(height);
        }

        Ok(prev)
    }
}

impl Database {
    pub fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.ids_of_latest_block()?
            .map(|(height, _)| height)
            .ok_or(not_found!("BlockHeight"))
    }

    /// Get the current block at the head of the chain.
    pub fn get_current_block(&self) -> StorageResult<Option<Cow<CompressedBlock>>> {
        let block_ids = self.ids_of_latest_block()?;
        match block_ids {
            Some((_, id)) => Ok(StorageAsRef::storage::<FuelBlocks>(self).get(&id)?),
            None => Ok(None),
        }
    }

    pub fn block_time(&self, height: &BlockHeight) -> StorageResult<Tai64> {
        let id = self.get_block_id(height)?.unwrap_or_default();
        let block = self
            .storage::<FuelBlocks>()
            .get(&id)?
            .ok_or(not_found!(FuelBlocks))?;
        Ok(block.header().time().to_owned())
    }

    pub fn get_block_id(&self, height: &BlockHeight) -> StorageResult<Option<BlockId>> {
        Database::get(
            self,
            height.database_key().as_ref(),
            Column::FuelBlockSecondaryKeyBlockHeights,
        )
        .map_err(Into::into)
    }

    pub fn all_block_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> impl Iterator<Item = DatabaseResult<(BlockHeight, BlockId)>> + '_ {
        let start = start.map(|b| b.to_bytes());
        self.iter_all_by_start::<Vec<u8>, BlockId, _>(
            Column::FuelBlockSecondaryKeyBlockHeights,
            start,
            Some(direction),
        )
        .map(|res| {
            let (height, id) = res?;
            let block_height_bytes: [u8; 4] = height
                .as_slice()
                .try_into()
                .expect("block height always has correct number of bytes");
            Ok((block_height_bytes.into(), id))
        })
    }

    pub fn ids_of_genesis_block(&self) -> DatabaseResult<(BlockHeight, BlockId)> {
        self.iter_all(
            Column::FuelBlockSecondaryKeyBlockHeights,
            Some(IterDirection::Forward),
        )
        .next()
        .ok_or(DatabaseError::ChainUninitialized)?
        .map(|(height, id): (Vec<u8>, BlockId)| {
            let bytes = <[u8; 4]>::try_from(height.as_slice())
                .expect("all block heights are stored with the correct amount of bytes");
            (u32::from_be_bytes(bytes).into(), id)
        })
    }

    pub fn ids_of_latest_block(&self) -> DatabaseResult<Option<(BlockHeight, BlockId)>> {
        let ids = self
            .iter_all::<Vec<u8>, BlockId>(
                Column::FuelBlockSecondaryKeyBlockHeights,
                Some(IterDirection::Reverse),
            )
            .next()
            .transpose()?
            .map(|(height, block)| {
                // safety: we know that all block heights are stored with the correct amount of bytes
                let bytes = <[u8; 4]>::try_from(height.as_slice()).unwrap();
                (u32::from_be_bytes(bytes).into(), block)
            });

        Ok(ids)
    }

    /// Retrieve the full block and all associated transactions
    pub(crate) fn get_full_block(
        &self,
        block_id: &BlockId,
    ) -> StorageResult<Option<Block>> {
        let db_block = self.storage::<FuelBlocks>().get(block_id)?;
        if let Some(block) = db_block {
            // fetch all the transactions
            // TODO: optimize with multi-key get
            let txs = block
                .transactions()
                .iter()
                .map(|tx_id| {
                    self.storage::<Transactions>()
                        .get(tx_id)
                        .and_then(|tx| tx.ok_or(not_found!(Transactions)))
                        .map(Cow::into_owned)
                })
                .try_collect()?;
            Ok(Some(block.into_owned().uncompress(txs)))
        } else {
            Ok(None)
        }
    }
}

impl MerkleRootStorage<BlockHeight, FuelBlocks> for Database {
    fn root(
        &self,
        key: &BlockHeight,
    ) -> Result<fuel_core_storage::MerkleRoot, Self::Error> {
        let metadata = self
            .storage::<FuelBlockMerkleMetadata>()
            .get(key)?
            .ok_or(not_found!(FuelBlocks))?;
        Ok(metadata.root)
    }
}

impl Database {
    pub fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof> {
        if message_block_height > commit_block_height {
            Err(anyhow::anyhow!(
                "The `message_block_height` is higher than `commit_block_height`"
            ))?;
        }

        let message_merkle_metadata = self
            .storage::<FuelBlockMerkleMetadata>()
            .get(message_block_height)?
            .ok_or(not_found!(FuelBlockMerkleMetadata))?;

        let commit_merkle_metadata = self
            .storage::<FuelBlockMerkleMetadata>()
            .get(commit_block_height)?
            .ok_or(not_found!(FuelBlockMerkleMetadata))?;

        let storage = self;
        let tree: MerkleTree<FuelBlockMerkleData, _> =
            MerkleTree::load(storage, commit_merkle_metadata.version)
                .map_err(|err| StorageError::Other(anyhow::anyhow!(err)))?;

        let proof_index = message_merkle_metadata
            .version
            .checked_sub(1)
            .ok_or(anyhow::anyhow!("The count of leafs - messages is zero"))?;
        let (_, proof_set) = tree
            .prove(proof_index)
            .map_err(|err| StorageError::Other(anyhow::anyhow!(err)))?;

        Ok(MerkleProof {
            proof_set,
            proof_index,
        })
    }
}

#[allow(clippy::arithmetic_side_effects)]
#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::{
        blockchain::{
            block::PartialFuelBlock,
            header::{
                ConsensusHeader,
                PartialBlockHeader,
            },
            primitives::Empty,
        },
        fuel_types::ChainId,
        fuel_vm::crypto::ephemeral_merkle_root,
    };
    use test_case::test_case;

    #[test_case(&[0]; "initial block with height 0")]
    #[test_case(&[1337]; "initial block with arbitrary height")]
    #[test_case(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]; "ten sequential blocks starting from height 0")]
    #[test_case(&[100, 101, 102, 103, 104, 105]; "five sequential blocks starting from height 100")]
    #[test_case(&[0, 2, 5, 7, 11]; "five non-sequential blocks starting from height 0")]
    #[test_case(&[100, 102, 105, 107, 111]; "five non-sequential blocks starting from height 100")]
    fn can_get_merkle_root_of_inserted_blocks(heights: &[u32]) {
        let mut database = Database::default();
        let blocks = heights
            .iter()
            .copied()
            .map(|height| {
                let header = PartialBlockHeader {
                    application: Default::default(),
                    consensus: ConsensusHeader::<Empty> {
                        height: height.into(),
                        ..Default::default()
                    },
                };
                let block = PartialFuelBlock::new(header, vec![]);
                block.generate(&[])
            })
            .collect::<Vec<_>>();

        // Insert the blocks. Each insertion creates a new version of Block
        // metadata, including a new root.
        for block in &blocks {
            StorageMutate::<FuelBlocks>::insert(
                &mut database,
                &block.id(),
                &block.compress(&ChainId::default()),
            )
            .unwrap();
        }

        // Check each version
        for version in 1..=blocks.len() {
            // Generate the expected root for the version
            let blocks = blocks.iter().take(version).collect::<Vec<_>>();
            let block_ids = blocks.iter().map(|block| block.id());
            let expected_root = ephemeral_merkle_root(block_ids);

            // Check that root for the version is present
            let last_block = blocks.last().unwrap();
            let actual_root = database
                .storage::<FuelBlocks>()
                .root(last_block.header().height())
                .expect("root to exist")
                .into();

            assert_eq!(expected_root, actual_root);
        }
    }

    #[test]
    fn get_merkle_root_with_no_blocks_returns_not_found_error() {
        let database = Database::default();

        // check that root is not present
        let err = database
            .storage::<FuelBlocks>()
            .root(&0u32.into())
            .expect_err("expected error getting invalid Block Merkle root");

        assert!(matches!(err, fuel_core_storage::Error::NotFound(_, _)));
    }

    const TEST_BLOCKS_COUNT: u32 = 10;

    fn insert_test_ascending_blocks(
        database: &mut Database,
        genesis_height: BlockHeight,
    ) {
        let start = *genesis_height;
        // Generate 10 blocks with ascending heights
        let blocks = (start..start + TEST_BLOCKS_COUNT)
            .map(|height| {
                let header = PartialBlockHeader {
                    application: Default::default(),
                    consensus: ConsensusHeader::<Empty> {
                        height: BlockHeight::from(height),
                        ..Default::default()
                    },
                };
                let block = PartialFuelBlock::new(header, vec![]);
                block.generate(&[])
            })
            .collect::<Vec<_>>();

        // Insert the blocks
        for block in &blocks {
            StorageMutate::<FuelBlocks>::insert(
                database,
                &block.id(),
                &block.compress(&ChainId::default()),
            )
            .unwrap();
        }
    }

    #[test]
    fn get_merkle_root_for_invalid_block_height_returns_not_found_error() {
        let mut database = Database::default();

        insert_test_ascending_blocks(&mut database, BlockHeight::from(0));

        // check that root is not present
        let err = database
            .storage::<FuelBlocks>()
            .root(&100u32.into())
            .expect_err("expected error getting invalid Block Merkle root");

        assert!(matches!(err, fuel_core_storage::Error::NotFound(_, _)));
    }

    #[test_case(0; "genesis block at height 0")]
    #[test_case(1; "genesis block at height 1")]
    #[test_case(100; "genesis block at height 100")]
    fn block_history_proof_works(genesis_height: u32) {
        let mut database = Database::default();

        insert_test_ascending_blocks(&mut database, BlockHeight::from(genesis_height));

        for l in 0..TEST_BLOCKS_COUNT {
            for r in l..TEST_BLOCKS_COUNT {
                let proof = database
                    .block_history_proof(
                        &BlockHeight::from(genesis_height + l),
                        &BlockHeight::from(genesis_height + r),
                    )
                    .expect("Should return the merkle proof");
                assert_eq!(proof.proof_index, l as u64);
            }
        }
    }

    #[test]
    fn block_history_proof_error_if_message_higher_than_commit() {
        let mut database = Database::default();

        insert_test_ascending_blocks(&mut database, BlockHeight::from(0));

        let result = database.block_history_proof(
            &BlockHeight::from(TEST_BLOCKS_COUNT),
            &BlockHeight::from(TEST_BLOCKS_COUNT - 1),
        );
        assert!(result.is_err());
    }
}
