use fuel_core_chain_config::{
    AddTable,
    AsTable,
    StateConfig,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    storage_interlayer::Interlayer,
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

/// Old blocks from before regenesis.
/// Has same form as [`FuelBlocks`](fuel_core_storage::tables::FuelBlocks).
pub struct OldFuelBlocks;

impl Mappable for OldFuelBlocks {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = CompressedBlock;
}

impl TableWithBlueprint for OldFuelBlocks {
    type Blueprint = Plain;
}

impl Interlayer for OldFuelBlocks {
    type KeyCodec = Primitive<4>;
    type ValueCodec = Postcard;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OldFuelBlocks
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    OldFuelBlocks,
    <OldFuelBlocks as Mappable>::Key::default(),
    <OldFuelBlocks as Mappable>::Value::default()
);

/// Old blocks from before regenesis.
/// Has same form as [`SealedBlockConsensus`](fuel_core_storage::tables::SealedBlockConsensus).
pub struct OldFuelBlockConsensus;

impl Mappable for OldFuelBlockConsensus {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = Consensus;
}

impl TableWithBlueprint for OldFuelBlockConsensus {
    type Blueprint = Plain;
}

impl Interlayer for OldFuelBlockConsensus {
    type KeyCodec = Primitive<4>;
    type ValueCodec = Postcard;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OldFuelBlockConsensus
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    OldFuelBlockConsensus,
    <OldFuelBlockConsensus as Mappable>::Key::default(),
    <OldFuelBlockConsensus as Mappable>::Value::default()
);

/// Old transactions from before regenesis.
/// Has same form as [`Transactions`](fuel_core_storage::tables::Transactions).
pub struct OldTransactions;

impl Mappable for OldTransactions {
    type Key = Self::OwnedKey;
    type OwnedKey = TxId;
    type Value = Self::OwnedValue;
    type OwnedValue = Transaction;
}

impl TableWithBlueprint for OldTransactions {
    type Blueprint = Plain;
}

impl Interlayer for OldTransactions {
    type KeyCodec = Raw;
    type ValueCodec = Postcard;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OldTransactions
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    OldTransactions,
    <OldTransactions as Mappable>::Key::default(),
    <OldTransactions as Mappable>::Value::default()
);

impl AsTable<OldFuelBlocks> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<OldFuelBlocks>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<OldFuelBlocks> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<OldFuelBlocks>>) {
        // Do not include these for now
    }
}

impl AsTable<OldFuelBlockConsensus> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<OldFuelBlockConsensus>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<OldFuelBlockConsensus> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<OldFuelBlockConsensus>>) {
        // Do not include these for now
    }
}

impl AsTable<OldTransactions> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<OldTransactions>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<OldTransactions> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<OldTransactions>>) {
        // Do not include these for now
    }
}
