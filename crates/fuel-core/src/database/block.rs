use crate::{
    database::{
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
    },
    state::IterDirection,
};
use fuel_core_storage::{
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
        primitives::{
            BlockHeight,
            BlockId,
        },
    },
    fuel_merkle::binary::MerkleTree,
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
        Database::exists(self, key.as_slice(), Column::FuelBlocks).map_err(Into::into)
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

        // get latest metadata entry
        let prev_metadata = self
            .iter_all::<Vec<u8>, DenseMerkleMetadata>(
                Column::FuelBlockMerkleMetadata,
                None,
                None,
                Some(IterDirection::Reverse),
            )
            .next()
            .transpose()?
            .map(|(_, metadata)| metadata)
            .unwrap_or_default();

        let storage = self.borrow_mut();
        let mut tree: MerkleTree<FuelBlockMerkleData, _> =
            MerkleTree::load(storage, prev_metadata.version)
                .map_err(|err| StorageError::Other(err.into()))?;
        let data = key.as_slice();
        tree.push(data)
            .map_err(|err| StorageError::Other(err.into()))?;

        // Generate new metadata for the updated tree
        let version = tree.leaves_count();
        let root = tree.root().into();
        let metadata = DenseMerkleMetadata { version, root };
        self.storage::<FuelBlockMerkleMetadata>()
            .insert(height, &metadata)?;

        Ok(prev)
    }

    fn remove(&mut self, key: &BlockId) -> Result<Option<CompressedBlock>, Self::Error> {
        let prev: Option<CompressedBlock> =
            Database::remove(self, key.as_slice(), Column::FuelBlocks)?;

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
        let start = start.map(|b| b.to_bytes().to_vec());
        self.iter_all::<Vec<u8>, BlockId>(
            Column::FuelBlockSecondaryKeyBlockHeights,
            None,
            start,
            Some(direction),
        )
        .map(|res| {
            let (height, id) = res?;
            Ok((
                height
                    .try_into()
                    .expect("block height always has correct number of bytes"),
                id,
            ))
        })
    }

    pub fn ids_of_genesis_block(&self) -> DatabaseResult<(BlockHeight, BlockId)> {
        self.iter_all(
            Column::FuelBlockSecondaryKeyBlockHeights,
            None,
            None,
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
                None,
                None,
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
            .ok_or(not_found!(FuelBlocks))
            .map(Cow::into_owned)?;
        Ok(metadata.root.into())
    }
}

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
        fuel_vm::crypto::ephemeral_merkle_root,
    };
    use test_case::test_case;

    #[test_case(&[0]; "initial block with height 0")]
    #[test_case(&[1337]; "initial block with arbitrary height")]
    #[test_case(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]; "ten sequential blocks starting from height 0")]
    #[test_case(&[100, 101, 102, 103, 104, 105]; "five sequential blocks starting from height 100")]
    #[test_case(&[0, 2, 5, 7, 11]; "five non-sequential blocks starting from height 0")]
    #[test_case(&[100, 102, 105, 107, 111]; "five non-sequential blocks starting from height 100")]
    fn can_get_merkle_root_of_inserted_blocks(heights: &[u64]) {
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
                &block.compress(),
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

    #[test]
    fn get_merkle_root_for_invalid_block_height_returns_not_found_error() {
        let mut database = Database::default();

        // Generate 10 blocks with ascending heights
        let blocks = (0u64..10)
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

        // Insert the blocks
        for block in &blocks {
            StorageMutate::<FuelBlocks>::insert(
                &mut database,
                &block.id(),
                &block.compress(),
            )
            .unwrap();
        }

        // check that root is not present
        let err = database
            .storage::<FuelBlocks>()
            .root(&100u32.into())
            .expect_err("expected error getting invalid Block Merkle root");

        assert!(matches!(err, fuel_core_storage::Error::NotFound(_, _)));
    }
}
