use crate::{
    column::TableColumn,
    merkle::MerkleizedTableColumn,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    merkle::column::{
        AsU32,
        MerkleizedColumn,
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

impl<Table, TC> TableWithBlueprint for MerkleData<Table>
where
    Table: MerkleizedTableColumn<TableColumn = TC>,
    TC: core::fmt::Debug + Copy + strum::EnumCount + AsU32,
{
    type Blueprint = Plain<Raw, Postcard>;
    type Column = MerkleizedColumn<TC>;

    fn column() -> Self::Column {
        Self::Column::MerkleDataColumn(Table::table_column())
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
    type Column = MerkleizedColumn<TableColumn>;

    fn column() -> Self::Column {
        Self::Column::MerkleMetadataColumn
    }
}
