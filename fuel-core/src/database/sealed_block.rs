use crate::database::{
    Column,
    Database,
};
use fuel_core_interfaces::{
    common::prelude::{
        Bytes32,
        StorageAsRef,
        StorageInspect,
        StorageMutate,
    },
    db::{
        FuelBlocks,
        KvStoreError,
        SealedBlockConsensus,
    },
    model::{
        FuelBlockConsensus,
        Genesis,
        SealedFuelBlock,
        SealedFuelBlockHeader,
    },
    not_found,
};
use std::borrow::Cow;

impl StorageInspect<SealedBlockConsensus> for Database {
    type Error = KvStoreError;

    fn get(
        &self,
        key: &Bytes32,
    ) -> Result<Option<Cow<FuelBlockConsensus>>, KvStoreError> {
        Database::get(self, key.as_ref(), Column::FuelBlockConsensus).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), Column::FuelBlockConsensus)
            .map_err(Into::into)
    }
}

impl StorageMutate<SealedBlockConsensus> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &FuelBlockConsensus,
    ) -> Result<Option<FuelBlockConsensus>, KvStoreError> {
        Database::insert(self, key.as_ref(), Column::FuelBlockConsensus, value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &Bytes32,
    ) -> Result<Option<FuelBlockConsensus>, KvStoreError> {
        Database::remove(self, key.as_ref(), Column::FuelBlockConsensus)
            .map_err(Into::into)
    }
}

impl Database {
    pub fn get_sealed_block(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<SealedFuelBlock>, KvStoreError> {
        // combine the block and consensus metadata into a sealed fuel block type

        let block = self.get_full_block(block_id)?;
        let consensus = self.storage::<SealedBlockConsensus>().get(block_id)?;

        if let (Some(block), Some(consensus)) = (block, consensus) {
            let sealed_block = SealedFuelBlock {
                block,
                consensus: consensus.into_owned(),
            };

            Ok(Some(sealed_block))
        } else {
            Ok(None)
        }
    }

    pub fn get_genesis(&self) -> Result<Genesis, KvStoreError> {
        let (_, genesis_block_id) = self.genesis_block_ids()?;
        let consensus = self
            .storage::<SealedBlockConsensus>()
            .get(&genesis_block_id)?
            .map(|c| c.into_owned());

        if let Some(FuelBlockConsensus::Genesis(genesis)) = consensus {
            Ok(genesis)
        } else {
            Err(not_found!(SealedBlockConsensus))
        }
    }

    pub fn get_sealed_block_header(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<SealedFuelBlockHeader>, KvStoreError> {
        let header = self.storage::<FuelBlocks>().get(block_id)?;
        let consensus = self.storage::<SealedBlockConsensus>().get(block_id)?;

        if let (Some(header), Some(consensus)) = (header, consensus) {
            let sealed_block = SealedFuelBlockHeader {
                header: header.into_owned().header,
                consensus: consensus.into_owned(),
            };

            Ok(Some(sealed_block))
        } else {
            Ok(None)
        }
    }
}
