use crate::{
    column::Column,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    tables::merkle::SparseMerkleMetadata,
    Mappable,
};
use fuel_core_types::fuel_merkle::sparse;

/// The table of SMT data for Contract state.
pub struct MerkleData<Table>(core::marker::PhantomData<Table>);

impl<Table> Mappable for MerkleData<Table> {
    type Key = [u8; 32];
    type OwnedKey = Self::Key;
    type Value = sparse::Primitive;
    type OwnedValue = Self::Value;
}

impl<Table> TableWithBlueprint for MerkleData<Table>
where
    Table: MerkleizedTableColumn,
{
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::MerkleDataColumn(Table::table_column())
    }
}

/// The metadata table for [`MerkleData`] table.
pub struct MerkleMetadata;

impl Mappable for MerkleMetadata {
    type Key = u32;
    type OwnedKey = Self::Key;
    type Value = SparseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for MerkleMetadata {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::MerkleMetadataColumn
    }
}
