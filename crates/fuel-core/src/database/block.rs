use crate::{
    database::{
        storage::ToDatabaseKey,
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
        FuelBlockMerkleData,
        FuelBlockMerkleMetadata,
        FuelBlockSecondaryKeyBlockHeights,
        FuelBlocks,
        Transactions,
    },
    Error as StorageError,
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
    entities::merkle::DenseMerkleMetadata,
    fuel_merkle::binary::MerkleTree,
    fuel_types::Bytes32,
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
            .insert(&height, &metadata)?;

        Ok(prev)
    }

    fn remove(&mut self, key: &BlockId) -> Result<Option<CompressedBlock>, Self::Error> {
        Database::remove(self, key.as_slice(), Column::FuelBlocks).map_err(Into::into)
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

    pub fn block_header_merkle_root(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Bytes32> {
        let metadata = self
            .storage::<FuelBlockMerkleMetadata>()
            .get(height)?
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
            header::PartialBlockHeader,
        },
        fuel_vm::crypto::ephemeral_merkle_root,
    };

    #[test]
    fn can_get_merkle_root_of_inserted_block() {
        let mut database = Database::default();

        let header = PartialBlockHeader {
            application: Default::default(),
            consensus: Default::default(),
        };
        let block = PartialFuelBlock::new(header, vec![]);
        let block = block.generate(&[]);

        // expected root
        let expected_root = ephemeral_merkle_root(vec![block.id().as_slice()].iter());

        // insert the block
        StorageMutate::<FuelBlocks>::insert(
            &mut database,
            &block.id(),
            &block.compress(),
        )
        .unwrap();

        // check that root is present
        let actual_root = database
            .block_header_merkle_root(block.header().height())
            .expect("root to exist");

        assert_eq!(expected_root, actual_root);
    }
}
