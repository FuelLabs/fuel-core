use crate::database::{
    Column,
    Database,
};
use fuel_core_database::Error as DatabaseError;
use fuel_core_storage::{
    not_found,
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
    },
    Error as StorageError,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Genesis,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Bytes32,
};
use std::borrow::Cow;

impl StorageInspect<SealedBlockConsensus> for Database {
    type Error = StorageError;

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Consensus>>, StorageError> {
        Database::get(self, key.as_ref(), Column::FuelBlockConsensus).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, StorageError> {
        Database::exists(self, key.as_ref(), Column::FuelBlockConsensus)
            .map_err(Into::into)
    }
}

impl StorageMutate<SealedBlockConsensus> for Database {
    fn insert(
        &mut self,
        key: &Bytes32,
        value: &Consensus,
    ) -> Result<Option<Consensus>, StorageError> {
        Database::insert(self, key.as_ref(), Column::FuelBlockConsensus, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Consensus>, StorageError> {
        Database::remove(self, key.as_ref(), Column::FuelBlockConsensus)
            .map_err(Into::into)
    }
}

impl Database {
    pub fn get_sealed_block(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<SealedBlock>, DatabaseError> {
        // combine the block and consensus metadata into a sealed fuel block type

        let block = self.get_full_block(block_id)?;
        let consensus = self.storage::<SealedBlockConsensus>().get(block_id)?;

        if let (Some(block), Some(consensus)) = (block, consensus) {
            let sealed_block = SealedBlock {
                entity: block,
                consensus: consensus.into_owned(),
            };

            Ok(Some(sealed_block))
        } else {
            Ok(None)
        }
    }

    pub fn get_genesis(&self) -> Result<Genesis, DatabaseError> {
        let (_, genesis_block_id) = self.genesis_block_ids()?;
        let consensus = self
            .storage::<SealedBlockConsensus>()
            .get(&genesis_block_id)?
            .map(|c| c.into_owned());

        if let Some(Consensus::Genesis(genesis)) = consensus {
            Ok(genesis)
        } else {
            Err(not_found!(SealedBlockConsensus)).map_err(Into::into)
        }
    }

    pub fn get_sealed_block_header(
        &self,
        block_id: &Bytes32,
    ) -> Result<Option<SealedBlockHeader>, DatabaseError> {
        let header = self.storage::<FuelBlocks>().get(block_id)?;
        let consensus = self.storage::<SealedBlockConsensus>().get(block_id)?;

        if let (Some(header), Some(consensus)) = (header, consensus) {
            let sealed_block = SealedBlockHeader {
                entity: header.into_owned().header().clone(),
                consensus: consensus.into_owned(),
            };

            Ok(Some(sealed_block))
        } else {
            Ok(None)
        }
    }
}
