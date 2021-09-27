use crate::database::columns::BLOCK_IDS;
use crate::database::{columns::BLOCKS, Database, KvStore, KvStoreError};
use crate::model::fuel_block::{BlockHeight, FuelBlock};
use crate::state::{Error, IterDirection};
use fuel_tx::Bytes32;
use std::convert::{TryFrom, TryInto};

impl KvStore<Bytes32, FuelBlock> for Database {
    fn insert(&self, key: &Bytes32, value: &FuelBlock) -> Result<Option<FuelBlock>, KvStoreError> {
        Database::insert(&self, value.fuel_height, BLOCK_IDS, *key)?;
        Database::insert(&self, key.as_ref(), BLOCKS, value.clone()).map_err(Into::into)
    }

    fn remove(&self, key: &Bytes32) -> Result<Option<FuelBlock>, KvStoreError> {
        let block: Option<FuelBlock> =
            Database::remove(&self, key.as_ref(), BLOCKS).map_err(|e| KvStoreError::from(e))?;
        if let Some(block) = &block {
            let _: Option<Bytes32> =
                Database::remove(&self, &block.fuel_height.to_bytes(), BLOCK_IDS)
                    .map_err(|e| KvStoreError::from(e))?;
        }
        Ok(block)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<FuelBlock>, KvStoreError> {
        Database::get(&self, key.as_ref(), BLOCKS).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(&self, key.as_ref(), BLOCKS).map_err(Into::into)
    }
}

impl Database {
    pub fn get_block_height(&self) -> Result<Option<BlockHeight>, Error> {
        let id: Option<(Vec<u8>, Bytes32)> = self
            .iter_all(BLOCK_IDS, None, None, Some(IterDirection::Reverse))
            .next()
            .transpose()?;

        // safety: we know that all block heights are stored with the correct amount of bytes
        Ok(id.map(|(height, _)| {
            let bytes = <[u8; 4]>::try_from(height.as_slice()).unwrap();
            u32::from_be_bytes(bytes).into()
        }))
    }

    pub fn get_block_id(&self, height: BlockHeight) -> Result<Option<Bytes32>, Error> {
        Database::get(&self, &height.to_bytes()[..], BLOCK_IDS)
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
