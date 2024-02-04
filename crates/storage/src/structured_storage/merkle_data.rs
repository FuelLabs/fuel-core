//! The module contains implementations and tests for merkle related tables.

use crate::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    column::Column,
    structured_storage::TableWithBlueprint,
    tables::merkle::{
        ContractsAssetsMerkleData,
        ContractsAssetsMerkleMetadata,
        ContractsStateMerkleData,
        ContractsStateMerkleMetadata,
        FuelBlockMerkleData,
        FuelBlockMerkleMetadata,
    },
};

macro_rules! merkle_table {
    ($table:ident) => {
        merkle_table!($table, Raw);
    };
    ($table:ident, $key_codec:ident) => {
        impl TableWithBlueprint for $table {
            type Blueprint = Plain<$key_codec, Postcard>;
            type Column = Column;

            fn column() -> Column {
                Column::$table
            }
        }

        #[cfg(test)]
        $crate::basic_storage_tests!(
            $table,
            <$table as $crate::Mappable>::Key::default(),
            <$table as $crate::Mappable>::Value::default()
        );
    };
}

type U64Codec = Primitive<8>;
type BlockHeightCodec = Primitive<4>;

merkle_table!(FuelBlockMerkleData, U64Codec);
merkle_table!(FuelBlockMerkleMetadata, BlockHeightCodec);
merkle_table!(ContractsAssetsMerkleData);
merkle_table!(ContractsAssetsMerkleMetadata);
merkle_table!(ContractsStateMerkleData);
merkle_table!(ContractsStateMerkleMetadata);
