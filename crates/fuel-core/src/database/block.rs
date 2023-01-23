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
use fuel_core_producer::ports::{
    BinaryMerkleMetadataStorage,
    BinaryMerkleTreeStorage,
    BlockExecutor,
};
use fuel_core_storage::{
    not_found,
    tables::{
        FuelBlockIds,
        FuelBlockMerkleData,
        FuelBlocks,
        FuelDenseMerkleMetadata,
        Transactions,
    },
    Error as StorageError,
    Mappable,
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
    fuel_merkle::binary::{
        MerkleTree,
        Primitive,
    },
    merkle::metadata::DenseMerkleMetadata,
    tai64::Tai64,
};
use itertools::Itertools;
use std::{
    borrow::{
        Borrow,
        Cow,
    },
    convert::{
        TryFrom,
        TryInto,
    },
};

impl StorageInspect<FuelBlockIds> for Database {
    type Error = StorageError;

    fn get(&self, key: &BlockHeight) -> Result<Option<Cow<BlockId>>, Self::Error> {
        Database::get(self, &key.to_be_bytes(), Column::FuelBlockIds).map_err(Into::into)
    }

    fn contains_key(&self, key: &BlockHeight) -> Result<bool, Self::Error> {
        Database::exists(self, &key.to_be_bytes(), Column::FuelBlockIds)
            .map_err(Into::into)
    }
}

impl StorageMutate<FuelBlockIds> for Database {
    fn insert(
        &mut self,
        key: &BlockHeight,
        value: &BlockId,
    ) -> Result<Option<BlockId>, Self::Error> {
        Database::insert(self, key.to_be_bytes(), Column::FuelBlockIds, *value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &BlockHeight) -> Result<Option<BlockId>, Self::Error> {
        Database::remove(self, &key.to_be_bytes(), Column::FuelBlockIds)
            .map_err(Into::into)
    }
}

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
        Database::insert(self, key.as_slice(), Column::FuelBlocks, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &BlockId) -> Result<Option<CompressedBlock>, Self::Error> {
        Database::remove(self, key.as_slice(), Column::FuelBlocks).map_err(Into::into)
    }
}

impl StorageInspect<FuelBlockMerkleData> for Database {
    type Error = StorageError;

    fn get(&self, key: &u64) -> Result<Option<Cow<Primitive>>, Self::Error> {
        Database::get(self, &key.to_be_bytes(), Column::FuelBlockMerkleData)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &u64) -> Result<bool, Self::Error> {
        Database::exists(self, &key.to_be_bytes(), Column::FuelBlockMerkleData)
            .map_err(Into::into)
    }
}

impl StorageMutate<FuelBlockMerkleData> for Database {
    fn insert(
        &mut self,
        key: &u64,
        value: &Primitive,
    ) -> Result<Option<Primitive>, Self::Error> {
        Database::insert(self, key.to_be_bytes(), Column::FuelBlockMerkleData, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &u64) -> Result<Option<Primitive>, Self::Error> {
        Database::remove(self, &key.to_be_bytes(), Column::FuelBlockMerkleData)
            .map_err(Into::into)
    }
}

impl StorageMutate<FuelBlockMerkleData> for &Database {
    fn insert(
        &mut self,
        key: &u64,
        value: &Primitive,
    ) -> Result<Option<Primitive>, Self::Error> {
        Database::insert(self, key.to_be_bytes(), Column::FuelBlockMerkleData, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &u64) -> Result<Option<Primitive>, Self::Error> {
        Database::remove(self, &key.to_be_bytes(), Column::FuelBlockMerkleData)
            .map_err(Into::into)
    }
}

impl StorageInspect<FuelDenseMerkleMetadata> for Database {
    type Error = StorageError;

    fn get(&self, key: &String) -> Result<Option<Cow<DenseMerkleMetadata>>, Self::Error> {
        Database::get(self, key.as_bytes(), Column::FuelBlockMerkleMetadata)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &String) -> Result<bool, Self::Error> {
        Database::exists(self, key.as_bytes(), Column::FuelBlockMerkleMetadata)
            .map_err(Into::into)
    }
}

impl StorageMutate<FuelDenseMerkleMetadata> for Database {
    fn insert(
        &mut self,
        key: &String,
        value: &DenseMerkleMetadata,
    ) -> Result<Option<DenseMerkleMetadata>, Self::Error> {
        Database::insert(self, key.as_bytes(), Column::FuelBlockMerkleMetadata, value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &String,
    ) -> Result<Option<DenseMerkleMetadata>, Self::Error> {
        Database::remove(self, key.as_bytes(), Column::FuelBlockMerkleMetadata)
            .map_err(Into::into)
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
        Database::get(self, height.database_key().as_ref(), Column::FuelBlockIds)
            .map_err(Into::into)
    }

    pub fn all_block_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> impl Iterator<Item = DatabaseResult<(BlockHeight, BlockId)>> + '_ {
        let start = start.map(|b| b.to_bytes().to_vec());
        self.iter_all::<Vec<u8>, BlockId>(
            Column::FuelBlockIds,
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
            Column::FuelBlockIds,
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
                Column::FuelBlockIds,
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

impl BlockExecutor for Database {
    fn insert_block(
        &mut self,
        block_id: &BlockId,
        block: &CompressedBlock,
    ) -> Result<(), StorageError> {
        self.storage::<FuelBlockIds>()
            .insert(block.header().height(), block_id)?;
        self.storage::<FuelBlocks>().insert(block_id, block)?;

        let data = block_id.as_slice();
        let metadata =
            self.load_binary_merkle_metadata(FUEL_BLOCK_MERKLE_METADATA_KEY)?;
        let mut tree =
            self.load_binary_merkle_tree::<FuelBlockMerkleData>(metadata.version)?;
        tree.push(data).unwrap();

        // Generate new metadata for the updated tree
        let version = tree.leaves_count();
        let root = tree.root().into();
        let metadata = DenseMerkleMetadata { version, root };
        self.save_binary_merkle_metadata(FUEL_BLOCK_MERKLE_METADATA_KEY, &metadata)?;

        Ok(())
    }
}

impl BinaryMerkleMetadataStorage for Database {
    fn load_binary_merkle_metadata(
        &self,
        key: &str,
    ) -> Result<DenseMerkleMetadata, StorageError> {
        let metadata = self
            .storage::<FuelDenseMerkleMetadata>()
            .get(&key.into())?
            .unwrap_or_default()
            .into_owned();
        Ok(metadata)
    }

    fn save_binary_merkle_metadata(
        &mut self,
        key: &str,
        metadata: &DenseMerkleMetadata,
    ) -> Result<(), StorageError> {
        self.storage::<FuelDenseMerkleMetadata>()
            .insert(&key.into(), &metadata)?;
        Ok(())
    }
}

impl BinaryMerkleTreeStorage for Database {
    fn load_binary_merkle_tree<Table>(
        &self,
        version: u64,
    ) -> Result<MerkleTree<Table, &Self>, StorageError>
    where
        Table: Mappable<Key = u64, SetValue = Primitive, GetValue = Primitive>,
        Self: StorageInspect<Table, Error = StorageError>,
    {
        let storage = self.borrow();
        let tree = MerkleTree::load(storage, version).unwrap();
        Ok(tree)
    }
}

const FUEL_BLOCK_MERKLE_METADATA_KEY: &str = "FuelBlocks";
