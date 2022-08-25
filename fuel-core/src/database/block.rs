use crate::{
    database::{
        columns::{
            BLOCKS,
            BLOCK_IDS,
        },
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
    fuel_storage::Storage,
    fuel_tx::Bytes32,
};
use std::{
    borrow::Cow,
    convert::{
        TryFrom,
        TryInto,
    },
};

impl Storage<Bytes32, FuelBlockDb> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &Bytes32,
        value: &FuelBlockDb,
    ) -> Result<Option<FuelBlockDb>, KvStoreError> {
        Database::insert(self, value.headers.height, BLOCK_IDS, *key)?;
        Database::insert(self, key.as_ref(), BLOCKS, value.clone()).map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<FuelBlockDb>, KvStoreError> {
        let block: Option<FuelBlockDb> = Database::remove(self, key.as_ref(), BLOCKS)?;
        if let Some(block) = &block {
            let _: Option<Bytes32> =
                Database::remove(self, &block.headers.height.to_bytes(), BLOCK_IDS)?;
        }
        Ok(block)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<FuelBlockDb>>, KvStoreError> {
        Database::get(self, key.as_ref(), BLOCKS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), BLOCKS).map_err(Into::into)
    }
}

impl Database {
    pub fn get_block_height(&self) -> Result<Option<BlockHeight>, Error> {
        let block_entry: Option<(Vec<u8>, Bytes32)> = self
            .iter_all(BLOCK_IDS, None, None, Some(IterDirection::Reverse))
            .next()
            .transpose()?;
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

    pub fn get_block_id(&self, height: BlockHeight) -> Result<Option<Bytes32>, Error> {
        Database::get(self, &height.to_bytes()[..], BLOCK_IDS)
    }

    pub fn all_block_ids(
        &self,
        start: Option<BlockHeight>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<(BlockHeight, Bytes32), Error>> + '_ {
        let start = start.map(|b| b.to_bytes().to_vec());
        self.iter_all::<Vec<u8>, Bytes32>(BLOCK_IDS, None, start, direction)
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
}
