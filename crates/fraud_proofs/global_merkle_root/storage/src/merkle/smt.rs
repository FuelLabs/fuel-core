use crate::{
    column::Columns,
    merkle::MerklizedTableColumn,
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
    Table: MerklizedTableColumn,
{
    type Blueprint = Plain<Raw, Postcard>;
    type Column = Columns;

    fn column() -> Columns {
        Columns::MerkleDataColumns(Table::table_column())
    }
}

/// The metadata table for [`MerkleData`] table.
pub struct MerkleMetadata<Table>(core::marker::PhantomData<Table>);

impl<Table> Mappable for MerkleMetadata<Table> {
    type Key = u32;
    type OwnedKey = Self::Key;
    type Value = SparseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl<Table> TableWithBlueprint for MerkleMetadata<Table>
where
    Table: MerklizedTableColumn,
{
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = Columns;

    fn column() -> Columns {
        Columns::MerkleDataColumns(Table::table_column())
    }
}
