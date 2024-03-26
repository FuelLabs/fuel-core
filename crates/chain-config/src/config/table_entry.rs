use fuel_core_storage::Mappable;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TableEntry<T>
where
    T: Mappable,
{
    pub key: T::OwnedKey,
    pub value: T::OwnedValue,
}

impl<T> Clone for TableEntry<T>
where
    T: Mappable,
{
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }
}

impl<T> Copy for TableEntry<T>
where
    T: Mappable,
    T::OwnedKey: Copy,
    T::OwnedValue: Copy,
{
}

impl<T> std::fmt::Debug for TableEntry<T>
where
    T: Mappable,
    T::OwnedKey: std::fmt::Debug,
    T::OwnedValue: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TableEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .finish()
    }
}

impl<T> PartialEq for TableEntry<T>
where
    T: Mappable,
    T::OwnedKey: PartialEq,
    T::OwnedValue: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}

impl<T> Eq for TableEntry<T>
where
    T: Mappable,
    T::OwnedKey: Eq,
    T::OwnedValue: Eq,
{
}

impl<T> PartialOrd for TableEntry<T>
where
    T: Mappable,
    T::OwnedKey: PartialOrd,
    T::OwnedValue: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.key.partial_cmp(&other.key) {
            Some(std::cmp::Ordering::Equal) => self.value.partial_cmp(&other.value),
            other => other,
        }
    }
}

impl<T> Ord for TableEntry<T>
where
    T: Mappable,
    T::OwnedKey: Ord,
    T::OwnedValue: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.key.cmp(&other.key) {
            std::cmp::Ordering::Equal => self.value.cmp(&other.value),
            other => other,
        }
    }
}
