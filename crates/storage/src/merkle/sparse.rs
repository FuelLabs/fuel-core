//! This module provides storage trait implementations for sparse merkleized columns.

use crate::{
    Mappable,
    Result as StorageResult,
    blueprint::{
        BlueprintInspect,
        plain::Plain,
        sparse::{
            PrimaryKey,
            Sparse,
        },
    },
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    merkle::column::{
        AsU32,
        MerkleizedColumn,
    },
    structured_storage::TableWithBlueprint,
    tables::merkle::SparseMerkleMetadata,
};
use alloc::borrow::Cow;
use fuel_core_types::fuel_merkle::sparse;

/// The table of SMT data
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
pub struct MerkleMetadata<TC>(core::marker::PhantomData<TC>);

impl<TC> Mappable for MerkleMetadata<TC> {
    type Key = u32;
    type OwnedKey = Self::Key;
    type Value = SparseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl<TC> TableWithBlueprint for MerkleMetadata<TC>
where
    TC: core::fmt::Debug + Copy + strum::EnumCount + AsU32,
{
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = MerkleizedColumn<TC>;

    fn column() -> Self::Column {
        Self::Column::MerkleMetadataColumn
    }
}

/// Marker type for key conversion logic
pub struct KeyConverter<Table>(core::marker::PhantomData<Table>);

impl<Table> PrimaryKey for KeyConverter<Table>
where
    Table: Mappable,
    Table: TableWithBlueprint,
{
    type InputKey = <Table as Mappable>::Key;
    type OutputKey = u32;

    fn primary_key(_: &Self::InputKey) -> Cow<Self::OutputKey> {
        Cow::Owned(Table::column().id())
    }
}

/// Merkleized wrapper of the inner table
pub struct Merkleized<Table>(core::marker::PhantomData<Table>);

/// Implementation of this trait for the table, inherits
/// the Merkle implementation for the [`Merkleized`] table.
pub trait MerkleizedTableColumn {
    /// The table column type
    type TableColumn;
    /// Get the table column
    fn table_column() -> Self::TableColumn;
}

impl<Table, TC> Mappable for Merkleized<Table>
where
    Table: Mappable + MerkleizedTableColumn<TableColumn = TC>,
{
    type Key = <Table as Mappable>::Key;
    type OwnedKey = <Table as Mappable>::OwnedKey;
    type Value = <Table as Mappable>::Value;
    type OwnedValue = <Table as Mappable>::OwnedValue;
}

type KeyCodec<Table, TC> =
    <<Table as TableWithBlueprint>::Blueprint as BlueprintInspect<
        Table,
        DummyStorage<MerkleizedColumn<TC>>,
    >>::KeyCodec;

type ValueCodec<Table, TC> =
    <<Table as TableWithBlueprint>::Blueprint as BlueprintInspect<
        Table,
        DummyStorage<MerkleizedColumn<TC>>,
    >>::ValueCodec;

impl<Table, TC> TableWithBlueprint for Merkleized<Table>
where
    Table: Mappable + MerkleizedTableColumn<TableColumn = TC>,
    Table: TableWithBlueprint,
    TC: core::fmt::Debug + Copy + strum::EnumCount + AsU32,
    Table::Blueprint: BlueprintInspect<Table, DummyStorage<MerkleizedColumn<TC>>>,
{
    type Blueprint = Sparse<
        KeyCodec<Table, TC>,
        ValueCodec<Table, TC>,
        MerkleMetadata<TC>,
        MerkleData<Table>,
        KeyConverter<Table>,
    >;
    type Column = MerkleizedColumn<TC>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Table::table_column())
    }
}

/// This storage exists to satisfy the compilation of the [`BlueprintInspect`].
/// But this types shouldn't exist at all, because `BlueprintInspect::KeyCodec`
/// and `BlueprintInspect::ValueCodec` shouldn't require a storage in a first place.
/// This is a workaround to satisfy the compiler for now until we refactor the `BlueprintInspect`.
#[derive(Default)]
pub struct DummyStorage<Column>(core::marker::PhantomData<Column>);

impl<Column> DummyStorage<Column> {
    /// Create a new dummy storage
    pub fn new() -> Self {
        Self(core::marker::PhantomData)
    }
}

impl<Column> KeyValueInspect for DummyStorage<Column>
where
    Column: StorageColumn,
{
    type Column = Column;

    fn get(&self, _: &[u8], _: Self::Column) -> StorageResult<Option<Value>> {
        unreachable!()
    }
}
