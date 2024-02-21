//! The module contains implementations and tests for the `FuelBlocks` table.

use crate::{
    blueprint::merklized::Merklized,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        Encode,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::{
        merkle::{
            FuelBlockMerkleData,
            FuelBlockMerkleMetadata,
        },
        FuelBlocks,
    },
};
use fuel_core_types::blockchain::block::CompressedBlock;
use fuel_vm_private::fuel_tx::Bytes32;

/// The encoder of `CompressedBlock` for the `FuelBlocks` table.
pub struct BlockEncoder;

impl Encode<CompressedBlock> for BlockEncoder {
    type Encoder<'a> = [u8; Bytes32::LEN];

    fn encode(value: &CompressedBlock) -> Self::Encoder<'_> {
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

    fn column() -> Column {
        Column::FuelBlocks
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        structured_storage::{
            test::InMemoryStorage,
            StructuredStorage,
            TableWithBlueprint,
        },
        tables::FuelBlocks,
        StorageAsMut,
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
    use fuel_vm_private::crypto::ephemeral_merkle_root;

    crate::basic_merklelized_storage_tests!(
        FuelBlocks,
        <FuelBlocks as crate::Mappable>::Key::default(),
        <FuelBlocks as crate::Mappable>::Value::default()
    );

    #[test_case::test_case(&[0]; "initial block with height 0")]
    #[test_case::test_case(&[1337]; "initial block with arbitrary height")]
    #[test_case::test_case(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]; "ten sequential blocks starting from height 0")]
    #[test_case::test_case(&[100, 101, 102, 103, 104, 105]; "five sequential blocks starting from height 100")]
    #[test_case::test_case(&[0, 2, 5, 7, 11]; "five non-sequential blocks starting from height 0")]
    #[test_case::test_case(&[100, 102, 105, 107, 111]; "five non-sequential blocks starting from height 100")]
    fn can_get_merkle_root_of_inserted_blocks(heights: &[u32]) {
        let mut storage =
            InMemoryStorage::<<FuelBlocks as TableWithBlueprint>::Column>::default();
        let mut database = StructuredStorage::new(&mut storage);
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
                block.generate(&[])
            })
            .collect::<Vec<_>>();

        // Insert the blocks. Each insertion creates a new version of Block
        // metadata, including a new root.
        for block in &blocks {
            StorageMutate::<FuelBlocks>::insert(
                &mut database,
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
            let actual_root = database
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
        let database = StructuredStorage::new(&storage);

        // check that root is not present
        let err = database
            .storage::<FuelBlocks>()
            .root(&0u32.into())
            .expect_err("expected error getting invalid Block Merkle root");

        assert!(matches!(err, crate::Error::NotFound(_, _)));
    }
}
