use crate::database::Database;
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
    tables::{
        FuelBlocks,
        SealedBlockConsensus,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Genesis,
            Sealed,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::Transactions,
};
use std::ops::Range;

impl Database {
    /// Returns `SealedBlock` by `height`.
    /// Reusable across different trait implementations
    pub fn get_sealed_block_by_height(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlock>> {
        // combine the block and consensus metadata into a sealed fuel block type
        let block = self.get_full_block(height)?;
        let consensus = self.storage::<SealedBlockConsensus>().get(height)?;

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

    pub fn get_genesis(&self) -> StorageResult<Genesis> {
        let pair = self
            .iter_all::<SealedBlockConsensus>(Some(IterDirection::Forward))
            .next()
            .transpose()?;

        if let Some((_, Consensus::Genesis(genesis))) = pair {
            Ok(genesis)
        } else {
            Err(not_found!(SealedBlockConsensus))
        }
    }

    pub fn get_sealed_block_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Vec<SealedBlockHeader>> {
        let headers = block_height_range
            .map(BlockHeight::from)
            .map(|height| self.get_sealed_block_header(&height))
            .collect::<StorageResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(headers)
    }

    pub fn get_sealed_block_header(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<SealedBlockHeader>> {
        let header = self.storage::<FuelBlocks>().get(height)?;
        let consensus = self.storage::<SealedBlockConsensus>().get(height)?;

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

    pub fn get_transactions_on_blocks(
        &self,
        block_height_range: Range<u32>,
    ) -> StorageResult<Option<Vec<Transactions>>> {
        let transactions = block_height_range
            .into_iter()
            .map(BlockHeight::from)
            .map(|block_height| {
                let transactions = self
                    .get_sealed_block_by_height(&block_height)?
                    .map(|Sealed { entity: block, .. }| block.into_inner().1)
                    .map(Transactions);
                Ok(transactions)
            })
            .collect::<StorageResult<_>>()?;
        Ok(transactions)
    }
}
