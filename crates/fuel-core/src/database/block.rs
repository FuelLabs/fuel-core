use crate::{
    database::{
        OffChainIterableKeyValueView,
        OnChainIterableKeyValueView,
    },
    fuel_core_graphql_api::storage::blocks::FuelBlockIdsToHeights,
};
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::{
        merkle::{
            DenseMetadataKey,
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
        },
        FuelBlocks,
        Transactions,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            CompressedBlock,
        },
        primitives::BlockId,
    },
    entities::relayer::message::MerkleProof,
    fuel_merkle::binary::MerkleTree,
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use std::borrow::Cow;

impl OffChainIterableKeyValueView {
    pub fn get_block_height(&self, id: &BlockId) -> StorageResult<Option<BlockHeight>> {
        self.storage::<FuelBlockIdsToHeights>()
            .get(id)
            .map(|v| v.map(|v| v.into_owned()))
    }
}

impl OnChainIterableKeyValueView {
    pub fn latest_compressed_block(&self) -> StorageResult<Option<CompressedBlock>> {
        let pair = self
            .iter_all::<FuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()?;

        Ok(pair.map(|(_, compressed_block)| compressed_block))
    }

    /// Get the current block at the head of the chain.
    pub fn get_current_block(&self) -> StorageResult<Option<CompressedBlock>> {
        self.latest_compressed_block()
    }

    /// Retrieve the full block and all associated transactions
    pub fn get_full_block(&self, height: &BlockHeight) -> StorageResult<Option<Block>> {
        let db_block = self.storage::<FuelBlocks>().get(height)?;
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

impl OnChainIterableKeyValueView {
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
            .get(&DenseMetadataKey::Primary(*message_block_height))?
            .ok_or(not_found!(FuelBlockMerkleMetadata))?;

        let commit_merkle_metadata = self
            .storage::<FuelBlockMerkleMetadata>()
            .get(&DenseMetadataKey::Primary(*commit_block_height))?
            .ok_or(not_found!(FuelBlockMerkleMetadata))?;

        let storage = self;
        let tree: MerkleTree<FuelBlockMerkleData, _> =
            MerkleTree::load(storage, commit_merkle_metadata.version())
                .map_err(|err| StorageError::Other(anyhow::anyhow!(err)))?;

        let proof_index = message_merkle_metadata
            .version()
            .checked_sub(1)
            .ok_or(anyhow::anyhow!("The count of leaves - messages is zero"))?;
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
    use crate::database::Database;
    use fuel_core_storage::{
        transactional::AtomicView,
        StorageMutate,
    };
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
    };
    use test_case::test_case;

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
                block.generate(&[], Default::default()).unwrap()
            })
            .collect::<Vec<_>>();

        // Insert the blocks
        for block in &blocks {
            StorageMutate::<FuelBlocks>::insert(
                database,
                block.header().height(),
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
        let view = database.latest_view().unwrap();

        for l in 0..TEST_BLOCKS_COUNT {
            for r in l..TEST_BLOCKS_COUNT {
                let proof = view
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
        let view = database.latest_view().unwrap();

        let result = view.block_history_proof(
            &BlockHeight::from(TEST_BLOCKS_COUNT),
            &BlockHeight::from(TEST_BLOCKS_COUNT - 1),
        );
        assert!(result.is_err());
    }
}
