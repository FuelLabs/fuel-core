use fuel_core_storage::Mappable;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyEntry<T>
where
    T: Mappable,
{
    pub key: T::OwnedKey,
    pub value: T::OwnedValue,
}
