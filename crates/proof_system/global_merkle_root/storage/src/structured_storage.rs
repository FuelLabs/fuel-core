use fuel_core_storage::Mappable;

mod blobs;
mod coins;
mod contracts;
mod messages;
mod transactions;
mod upgrades;

/// Wrapper over an existing table to allow implementing new foreign traits on it.
pub struct LocalMirror<Table>(core::marker::PhantomData<Table>);

impl<Table: Mappable> Mappable for LocalMirror<Table> {
    type Key = Table::Key;
    type OwnedKey = Table::OwnedKey;
    type OwnedValue = Table::OwnedValue;
    type Value = Table::Value;
}
