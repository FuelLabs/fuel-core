//! The module contains implementations and tests for merkle related tables.

use crate::{
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
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
        impl TableWithStructure for $table {
            type Structure = Plain<$key_codec, Postcard>;

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
