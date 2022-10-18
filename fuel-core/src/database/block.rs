use crate::{
    database::{
        storage::FuelBlocks,
        Column,
        Database,
        KvStoreError,
    },
    model::{
        BlockHeight,
        FuelBlockDb,
    },
    state::{
        Error,
        IterDirection,
    },
};
use fuel_core_interfaces::common::{
    fuel_storage::{
        StorageInspect,
        StorageMutate,
    },
    fuel_tx::Bytes32,
    prelude::StorageAsRef,
};
use std::{
    borrow::Cow,
    convert::{
        TryFrom,
        TryInto,
    },
};

impl StorageInspect<FuelBlocks> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<FuelBlockDb>>, KvStoreError> {
        Database::get(self, key.as_ref(), Column::FuelBlocks).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), Column::FuelBlocks).map_err(Into::into)
    }
}

impl StorageMutate<FuelBlocks> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &FuelBlockDb,
    ) -> Result<Option<FuelBlockDb>, KvStoreError> {
        let _: Option<BlockHeight> = Database::insert(
            self,
            value.header.height().to_be_bytes(),
            Column::FuelBlockIds,
            *key,
        )?;
        Database::insert(self, key.as_ref(), Column::FuelBlocks, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<FuelBlockDb>, KvStoreError> {
        let block: Option<FuelBlockDb> =
            Database::remove(self, key.as_ref(), Column::FuelBlocks)?;
        if let Some(block) = &block {
            let _: Option<Bytes32> = Database::remove(
                self,
                &block.header.height().to_be_bytes(),
                Column::FuelBlockIds,
            )?;
        }
        Ok(block)
    }
}

impl Database {
    pub fn get_block_height(&self) -> Result<Option<BlockHeight>, Error> {
        let block_entry = self.latest_block()?;
        // get block height from most recently indexed block
        let mut id = block_entry.map(|(height, _)| {
            // safety: we know that all block heights are stored with the correct amount of bytes
            let bytes = <[u8; 4]>::try_from(height.as_slice()).unwrap();
            u32::from_be_bytes(bytes).into()
        });
        // if no blocks, check if chain was configured with a base height
        if id.is_none() {
            id = self.get_starting_chain_height()?;
        }
        Ok(id)
    }

    /// Get the current block at the head of the chain.
    pub fn get_current_block(&self) -> Result<Option<Cow<FuelBlockDb>>, Error> {
        let block_entry = self.latest_block()?;
        match block_entry {
            Some((_, id)) => StorageAsRef::storage::<FuelBlocks>(self)
                .get(&id)
                .map_err(Error::from),
            None => Ok(None),
        }
    }

    pub fn get_block_id(&self, height: BlockHeight) -> Result<Option<Bytes32>, Error> {
        Database::get(self, &height.to_bytes()[..], Column::FuelBlockIds)
    }

    pub fn all_block_ids(
        &self,
        start: Option<BlockHeight>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<(BlockHeight, Bytes32), Error>> + '_ {
        let start = start.map(|b| b.to_bytes().to_vec());
        self.iter_all::<Vec<u8>, Bytes32>(Column::FuelBlockIds, None, start, direction)
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

    fn latest_block(&self) -> Result<Option<(Vec<u8>, Bytes32)>, Error> {
        self.iter_all(
            Column::FuelBlockIds,
            None,
            None,
            Some(IterDirection::Reverse),
        )
        .next()
        .transpose()
    }
}
