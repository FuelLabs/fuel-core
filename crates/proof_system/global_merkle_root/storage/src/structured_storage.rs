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
    type Key = <Table as Mappable>::Key;
    type OwnedKey = <Table as Mappable>::OwnedKey;
    type OwnedValue = <Table as Mappable>::OwnedValue;
    type Value = <Table as Mappable>::Value;
}
