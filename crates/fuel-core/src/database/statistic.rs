use crate::{
    database::{
        database_description::off_chain::OffChain,
        storage::UseStructuredImplementation,
        Database,
    },
    fuel_core_graphql_api,
    state::DataSource,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    Mappable,
    Result as StorageResult,
    StorageMutate,
};

/// The table that stores all statistic about blockchain. Each key is a string, while the value
/// depends on the context.
pub struct StatisticTable<V>(core::marker::PhantomData<V>);

impl<V> Mappable for StatisticTable<V>
where
    V: Clone,
{
    type Key = str;
    type OwnedKey = String;
    type Value = V;
    type OwnedValue = V;
}

impl<V> TableWithBlueprint for StatisticTable<V>
where
    V: Clone,
{
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = fuel_core_graphql_api::storage::Column;

    fn column() -> Self::Column {
        Self::Column::Statistic
    }
}

impl<V> UseStructuredImplementation<StatisticTable<V>>
    for StructuredStorage<DataSource<OffChain>>
where
    V: Clone,
{
}

/// Tracks the total number of transactions written to the chain
/// It's useful for analyzing TPS or other metrics.
pub(crate) const TX_COUNT: &str = "total_tx_count";

impl Database<OffChain> {
    pub fn increase_tx_count(&mut self, new_txs: u64) -> StorageResult<u64> {
        use fuel_core_storage::StorageAsRef;
        // TODO: how should tx count be initialized after regenesis?
        let current_tx_count: u64 = self
            .storage::<StatisticTable<u64>>()
            .get(TX_COUNT)?
            .unwrap_or_default()
            .into_owned();
        // Using saturating_add because this value doesn't significantly impact the correctness of execution.
        let new_tx_count = current_tx_count.saturating_add(new_txs);
        <_ as StorageMutate<StatisticTable<u64>>>::insert(
            &mut self.data,
            TX_COUNT,
            &new_tx_count,
        )?;
        Ok(new_tx_count)
    }
}
