use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    storage_interlayer::Interlayer,
    structured_storage::TableWithBlueprint,
    Mappable,
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
    type Blueprint = Plain;
}

impl<V> Interlayer for StatisticTable<V>
where
    V: Clone + serde::Serialize + serde::de::DeserializeOwned,
{
    type KeyCodec = Postcard;
    type ValueCodec = Postcard;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::Statistic
    }
}
