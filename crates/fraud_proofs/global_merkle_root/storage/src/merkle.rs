use crate::column::{
    Column,
    TableColumn,
};
use alloc::borrow::Cow;
use fuel_core_storage::{
    blueprint::{
        sparse::{
            PrimaryKey,
            Sparse,
        },
        BlueprintInspect,
    },
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
    Result as StorageResult,
};

mod smt;

pub type MerkleMetadata<T> = smt::MerkleMetadata<T>;
pub type MerkleData<T> = smt::MerkleData<T>;

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

pub struct Merklized<Table>(core::marker::PhantomData<Table>);

/// Implementation of this trait for the table, inherits
/// the Merkle implementation for the [`Merklized`] table.
pub trait MerklizedTableColumn {
    fn table_column() -> TableColumn;
}

impl<Table> Mappable for Merklized<Table>
where
    Table: Mappable + MerklizedTableColumn,
{
    type Key = <Table as Mappable>::Key;
    type OwnedKey = <Table as Mappable>::OwnedKey;
    type Value = <Table as Mappable>::Value;
    type OwnedValue = <Table as Mappable>::OwnedValue;
}

type KeyCodec<Table> = <<Table as TableWithBlueprint>::Blueprint as BlueprintInspect<
    Table,
    DummyStorage<Column>,
>>::KeyCodec;

type ValueCodec<Table> = <<Table as TableWithBlueprint>::Blueprint as BlueprintInspect<
    Table,
    DummyStorage<Column>,
>>::ValueCodec;

impl<Table> TableWithBlueprint for Merklized<Table>
where
    Table: Mappable + MerklizedTableColumn,
    Table: TableWithBlueprint,
    Table::Blueprint: BlueprintInspect<Table, DummyStorage<Column>>,
{
    type Blueprint = Sparse<
        KeyCodec<Table>,
        ValueCodec<Table>,
        MerkleMetadata<Table>,
        MerkleData<Table>,
        KeyConverter<Table>,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Column::TableColumn(Table::table_column())
    }
}

/// This storage exists to satisfy the compilation of the [`BlueprintInspect`].
/// But this types shouldn't exist at all, because `BlueprintInspect::KeyCodec`
/// and `BlueprintInspect::ValueCodec` shouldn't require a storage in a first place.
/// This is a workaround to satisfy the compiler for now until we refactor the `BlueprintInspect`.
#[derive(Default)]
pub struct DummyStorage<Column>(core::marker::PhantomData<Column>);

impl<Column> DummyStorage<Column> {
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
