//! The module contains implementations and tests for the `FuelBlocks` table.

use crate::{
    blueprint::merklized::Merklized,
    codec::{
        Encode,
        postcard::Postcard,
        primitive::Primitive,
    },
    column::Column,
    tables::{
        FuelBlocks,
        merkle::{
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
        },
    },
};
use fuel_core_types::blockchain::block::{
    Block,
    CompressedBlock,
};
use fuel_vm_private::fuel_tx::Bytes32;

use super::TableWithBlueprint;

/// The encoder of `CompressedBlock` for the `FuelBlocks` table.
pub struct BlockEncoder;

impl Encode<CompressedBlock> for BlockEncoder {
    type Encoder<'a> = [u8; Bytes32::LEN];

    fn encode(value: &CompressedBlock) -> Self::Encoder<'_> {
        let bytes: Bytes32 = value.id().into();
        bytes.into()
    }
}

impl Encode<Block> for BlockEncoder {
    type Encoder<'a> = [u8; Bytes32::LEN];

    fn encode(value: &Block) -> Self::Encoder<'_> {
        let bytes: Bytes32 = value.id().into();
        bytes.into()
    }
}

impl TableWithBlueprint for FuelBlocks {
    type Blueprint = Merklized<
        Primitive<4>,
        Postcard,
        FuelBlockMerkleMetadata,
        FuelBlockMerkleData,
        BlockEncoder,
    >;

    type Column = Column;

    fn column() -> Self::Column {
        Column::FuelBlocks
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::{
        StorageAsMut,
        StorageMutate,
        blueprint::merklized::basic_tests_bmt::{
            BMTTestDataGenerator,
            Wrapper,
        },
        structured_storage::{
            TableWithBlueprint,
            test::InMemoryStorage,
        },
        transactional::ReadTransaction,
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
    use fuel_vm_private::{
        crypto::ephemeral_merkle_root,
        fuel_storage::Mappable,
    };
    use rand::{
        Rng,
        rngs::StdRng,
    };

    impl BMTTestDataGenerator for FuelBlocks {
        type Key = <FuelBlocks as Mappable>::Key;
        type Value = Wrapper<<FuelBlocks as Mappable>::OwnedValue>;

        fn key() -> Self::Key {
            fuel_vm_private::fuel_types::BlockHeight::new(0)
        }

        fn random_key(rng: &mut StdRng) -> Self::Key {
            rng.r#gen()
        }

        fn generate_value(rng: &mut StdRng) -> Self::Value {
            let header = PartialBlockHeader {
                application: Default::default(),
                consensus: ConsensusHeader::<Empty> {
                    height: rng.r#gen(),
                    ..Default::default()
                },
            };
            let block = PartialFuelBlock::new(header, vec![]);
            let block = block
                .generate(
                    &[],
                    Default::default(),
                    #[cfg(feature = "fault-proving")]
                    &Default::default(),
                )
                .expect("The block is valid")
                .compress(&Default::default());

            Wrapper(block)
        }
    }

    crate::basic_merklized_storage_tests!(FuelBlocks);

    #[test_case::test_case(&[0]; "initial block with height 0")]
    #[test_case::test_case(&[1337]; "initial block with arbitrary height")]
    #[test_case::test_case(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]; "ten sequential blocks starting from height 0")]
    #[test_case::test_case(&[100, 101, 102, 103, 104, 105]; "five sequential blocks starting from height 100")]
    #[test_case::test_case(&[0, 2, 5, 7, 11]; "five non-sequential blocks starting from height 0")]
    #[test_case::test_case(&[100, 102, 105, 107, 111]; "five non-sequential blocks starting from height 100")]
    fn can_get_merkle_root_of_inserted_blocks(heights: &[u32]) {
        let storage =
            InMemoryStorage::<<FuelBlocks as TableWithBlueprint>::Column>::default();
        let mut storage = storage.read_transaction();
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
                block
                    .generate(
                        &[],
                        Default::default(),
                        #[cfg(feature = "fault-proving")]
                        &Default::default(),
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();

        // Insert the blocks. Each insertion creates a new version of Block
        // metadata, including a new root.
        for block in &blocks {
            StorageMutate::<FuelBlocks>::insert(
                &mut storage,
                block.header().height(),
                &block.compress(&ChainId::default()),
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
            let actual_root = storage
                .storage::<FuelBlocks>()
                .root(last_block.header().height())
                .expect("root to exist")
                .into();

            assert_eq!(expected_root, actual_root);
        }
    }

    #[test]
    fn get_merkle_root_with_no_blocks_returns_not_found_error() {
        use crate::StorageAsRef;

        let storage =
            InMemoryStorage::<<FuelBlocks as TableWithBlueprint>::Column>::default();
        let database = storage.read_transaction();

        // check that root is not present
        let err = database
            .storage::<FuelBlocks>()
            .root(&0u32.into())
            .expect_err("expected error getting invalid Block Merkle root");

        assert!(matches!(err, crate::Error::NotFound(_, _)));
    }
}
