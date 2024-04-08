use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    blockchain::block::CompressedBlock,
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
    type Blueprint = Plain<Postcard, Postcard>;
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
    type Blueprint = Plain<Raw, Postcard>;
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
