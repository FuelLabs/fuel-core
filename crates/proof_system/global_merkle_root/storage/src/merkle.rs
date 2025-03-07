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
    merkle::column::{
        AsU32,
        MerkleizedColumn,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
    Result as StorageResult,
};

mod smt;

/// The merkle metadata table containing the global root
pub type MerkleMetadata = smt::MerkleMetadata;
/// Template over merkle tree data tables
pub type MerkleData<T> = smt::MerkleData<T>;

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
        MerkleMetadata,
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
