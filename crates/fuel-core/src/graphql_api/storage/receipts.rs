use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_tx::{
    Bytes32,
    Receipt,
};

/// Receipts of different hidden internal operations.
pub struct Receipts;

impl Mappable for Receipts {
    /// Unique identifier of the transaction.
    type Key = Self::OwnedKey;
    type OwnedKey = Bytes32;
    type Value = [Receipt];
    type OwnedValue = Vec<Receipt>;
}

impl TableWithBlueprint for Receipts {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::Receipts
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    Receipts,
    <Receipts as Mappable>::Key::from([1u8; 32]),
    vec![Receipt::ret(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default()
    )]
);
