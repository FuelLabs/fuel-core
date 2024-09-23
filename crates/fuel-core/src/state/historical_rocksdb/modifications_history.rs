use std::borrow::Cow;

use crate::{
    database::database_description::DatabaseDescription,
    state::historical_rocksdb::description::Column,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    kv_store::{
        KeyValueInspect,
        KeyValueMutate,
    },
    structured_storage::{
        StructuredStorage,
        TableWithBlueprint,
    },
    transactional::Changes,
    Mappable,
    StorageInspect,
    StorageMutate,
};

/// ModificationsHystory. The `const N: usize` generic parameter
/// is used to specify different versions of the modificaiton history.
/// This allows to define different layout implementations for different
/// versions of the storage history (e.g. by defining different TableWithBlueprint
/// implementations for V1 and V2 of the modification history.
/// The [`ModificationHistoryVersion`]` struct is private. This forces reads and writes
/// to the modification history to go through the [`ModificationHistory`] struct,
/// for which storage inspection and mutation primitives are defined via the [`VersionedStorage`].
struct ModificationsHistoryVersion<Description, const N: usize>(
    core::marker::PhantomData<Description>,
)
where
    Description: DatabaseDescription;

pub struct ModificationsHistory<Description>(core::marker::PhantomData<Description>)
where
    Description: DatabaseDescription;

/// ModificationsHistory Key-value pairs.
impl<Description, const N: usize> Mappable for ModificationsHistoryVersion<Description, N>
where
    Description: DatabaseDescription,
{
    /// The height of the modifications.
    type Key = u64;
    type OwnedKey = Self::Key;
    /// Reverse modification at the corresponding height.
    type Value = Changes;
    type OwnedValue = Self::Value;
}

/// ModificationsHistory Key-value pairs.
impl<Description> Mappable for ModificationsHistory<Description>
where
    Description: DatabaseDescription,
{
    /// The height of the modifications.
    type Key = u64;
    type OwnedKey = Self::Key;
    /// Reverse modification at the corresponding height.
    type Value = Changes;
    type OwnedValue = Self::Value;
}

/// Blueprint for Modifications History V1. Keys are stored in little endian
/// using the `Column::HistoryColumn` column family.
impl<Description> TableWithBlueprint for ModificationsHistoryVersion<Description, 0>
where
    Description: DatabaseDescription,
{
    //  the keys in the database. https://github.com/FuelLabs/fuel-core/issues/2095
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column<Description>;

    fn column() -> Self::Column {
        Column::HistoryColumn
    }
}

/// Blueprint for Modificaations History V2. Keys are stored in little endian
/// using the `Column::HistoryV2Column` column family.
impl<Description> TableWithBlueprint for ModificationsHistoryVersion<Description, 1>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column<Description>;

    // We store key-value pairs using the same column family as for V1
    fn column() -> Self::Column {
        Column::HistoryV2Column
    }
}

// TODO: Remove this data structure! It is currently needed to allow reading
// and writing to the modifications history using the V1 of the key encoding.
impl<Description> TableWithBlueprint for ModificationsHistory<Description>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column<Description>;

    // We store key-value pairs using the same column family as for V1
    fn column() -> Self::Column {
        Column::HistoryV2Column
    }
}

type ModificationsHistoryV1<Description> = ModificationsHistoryVersion<Description, 0>;
type ModificationsHistoryV2<Description> = ModificationsHistoryVersion<Description, 1>;

/// [`VersionedStorage<S>`] wraps a StructuredStorage and adds versioning capabilities for the
/// ModificationsHistory.
/// - Reading a value for the `ModificationsHistory` corresponds to reading first from V2,
///   then falling back to V1 if an entry is not found
/// - Checking if a key exists follows the same logic as reading a key-value pair
/// - Removing a key-value pair from the `ModificationsHistory` is implemented by first removing
///   the key-value pair from the `ModificationsHistoryV1`, then from `ModificationsHistoryV2`.
///   The ordering of these operations guarantees that we always read the most up-to-date version
///   of a key-value pair, in the case of race conditions between reads and writes
/// - Replacing a key-value corresponds to reading the key-value pair through a (versioned) read
///   operation, overwriting the V2 value and then deleting the corresponding key-value pair
///   in the `ModicationsHistoryV1`.
pub struct VersionedStorage<S> {
    pub inner: StructuredStorage<S>,
}

impl<Description, S> StorageInspect<ModificationsHistory<Description>>
    for VersionedStorage<S>
where
    S: KeyValueInspect<Column = Column<Description>>,
    Description: DatabaseDescription,
{
    type Error = fuel_core_storage::Error;

    fn get(&self, key: &u64) -> Result<Option<Cow<Changes>>, Self::Error> {
        // Give priority to keys stored using the V2 format

        let v2 = <StructuredStorage<S> as StorageInspect<
            ModificationsHistoryV2<Description>,
        >>::get(&self.inner, key)?;

        match v2 {
            None => <StructuredStorage<S> as StorageInspect<
                ModificationsHistoryV1<Description>,
            >>::get(&self.inner, key),
            v2 => Ok(v2),
        }
    }

    fn contains_key(&self, key: &u64) -> Result<bool, Self::Error> {
        let exists_v2 = <StructuredStorage<S> as StorageInspect<
            ModificationsHistoryVersion<Description, 0>,
        >>::contains_key(&self.inner, key)?;
        if exists_v2 {
            Ok(true)
        } else {
            <StructuredStorage<S> as StorageInspect<
                ModificationsHistoryVersion<Description, 1>,
            >>::contains_key(&self.inner, key)
        }
    }
}

impl<S> VersionedStorage<S> {
    /// Creates a new instance of the structured storage.
    pub fn new(storage: S) -> Self {
        Self {
            inner: StructuredStorage::new(storage),
        }
    }

    /// Returns the inner storage.
    pub fn into_inner(self) -> S {
        self.inner.into_inner()
    }
}

impl<S> AsRef<S> for VersionedStorage<S> {
    fn as_ref(&self) -> &S {
        self.inner.as_ref()
    }
}

impl<S> AsMut<S> for VersionedStorage<S> {
    fn as_mut(&mut self) -> &mut S {
        self.inner.as_mut()
    }
}

impl<S> AsRef<StructuredStorage<S>> for VersionedStorage<S> {
    fn as_ref(&self) -> &StructuredStorage<S> {
        &self.inner
    }
}

impl<S> AsMut<StructuredStorage<S>> for VersionedStorage<S> {
    fn as_mut(&mut self) -> &mut StructuredStorage<S> {
        &mut self.inner
    }
}

impl<Description, S> StorageMutate<ModificationsHistory<Description>>
    for VersionedStorage<S>
where
    S: KeyValueMutate<Column = Column<Description>>,
    Description: DatabaseDescription,
{
    fn replace(
        &mut self,
        key: &u64,
        value: &Changes,
    ) -> Result<Option<Changes>, Self::Error> {
        // Read the most up-to-date value from either V1 or V2
        let old_value = self.get(key)?.map(|borrowed| borrowed.into_owned());
        // Write the new value using the V2 format
        <StructuredStorage<S> as StorageMutate<
                ModificationsHistoryV2<Description>,
            >>::replace(&mut self.inner, key, value)?;

        // Remove the key-value pair from V1, if any.
        <StructuredStorage<S> as StorageMutate<
                ModificationsHistoryV1<Description>,
            >>::take(&mut self.inner, key)?;

        Ok(old_value)
    }

    fn take(&mut self, key: &u64) -> Result<Option<Changes>, Self::Error> {
        // Check if there is a key-value pair for V2. This is not necessarily the most up-to-date value,
        // as there might be a key-value pair for the same key in V2.
        let v1_value_opt = <StructuredStorage<S> as StorageMutate<
            ModificationsHistoryV1<Description>,
        >>::take(&mut self.inner, key)?;
        let v2_value_opt = <StructuredStorage<S> as StorageMutate<
            ModificationsHistoryV2<Description>,
        >>::take(&mut self.inner, key)?;
        Ok(v2_value_opt.or(v1_value_opt))
    }
}

#[cfg(feature = "test-helpers")]
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fuel_core_storage::{
        structured_storage::{
            test::InMemoryStorage,
            StructuredStorage,
        },
        transactional::Changes,
        StorageInspect,
        StorageMutate,
    };

    use crate::{
        database::database_description::on_chain::OnChain,
        state::historical_rocksdb::{
            description::Column,
            modifications_history::{
                ModificationsHistory,
                ModificationsHistoryV2,
            },
        },
    };

    fn with_empty_level(changes: &mut Changes, height: u32) {
        changes.insert(height, BTreeMap::default());
    }

    use super::{
        ModificationsHistoryV1,
        VersionedStorage,
    };

    #[test]
    fn get_prefers_v2() {
        // Given
        let storage: InMemoryStorage<Column<OnChain>> = InMemoryStorage::default();
        let mut versioned_storage = VersionedStorage::new(storage);
        let as_structured_storage: &mut StructuredStorage<
            InMemoryStorage<Column<OnChain>>,
        > = versioned_storage.as_mut();
        let mut v1_value = Changes::default();
        with_empty_level(&mut v1_value, 42);
        let v2_value = Changes::default();

        // When
        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV1<OnChain>,
        >>::insert(as_structured_storage, &1, &v1_value)
        .unwrap();

        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV2<OnChain>,
        >>::insert(as_structured_storage, &1, &v2_value)
        .unwrap();

        // Then
        let fetched =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistory<OnChain>,
            >>::get(&versioned_storage, &1)
            .unwrap()
            .unwrap()
            .into_owned();

        assert!(fetched != v1_value);
        assert_eq!(fetched, v2_value);
    }

    #[test]
    fn get_returns_v1_on_old_values() {
        // Given
        let storage: InMemoryStorage<Column<OnChain>> = InMemoryStorage::default();
        let mut versioned_storage = VersionedStorage::new(storage);
        let as_structured_storage: &mut StructuredStorage<
            InMemoryStorage<Column<OnChain>>,
        > = versioned_storage.as_mut();
        let mut v1_value = Changes::default();
        with_empty_level(&mut v1_value, 42);

        // When

        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV1<OnChain>,
        >>::insert(as_structured_storage, &1, &v1_value)
        .unwrap();

        // Then
        let fetched =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistory<OnChain>,
            >>::get(&versioned_storage, &1)
            .unwrap()
            .unwrap()
            .into_owned();

        assert_eq!(fetched, v1_value);
    }

    #[test]
    fn take_removes_all_versions() {
        // Given
        let storage: InMemoryStorage<Column<OnChain>> = InMemoryStorage::default();
        let mut versioned_storage = VersionedStorage::new(storage);
        let as_structured_storage: &mut StructuredStorage<
            InMemoryStorage<Column<OnChain>>,
        > = versioned_storage.as_mut();
        let mut v1_value = Changes::default();
        with_empty_level(&mut v1_value, 42);
        let v2_value = Changes::default();

        // When
        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV1<OnChain>,
        >>::insert(as_structured_storage, &1, &v1_value)
        .unwrap();

        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV2<OnChain>,
        >>::insert(as_structured_storage, &1, &v2_value)
        .unwrap();

        let taken =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
                ModificationsHistory<OnChain>,
            >>::take(&mut versioned_storage, &1)
            .unwrap()
            .unwrap();

        let fetched_after_taken =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistory<OnChain>,
            >>::get(&versioned_storage, &1)
            .unwrap();

        let as_structured_storage: &StructuredStorage<InMemoryStorage<Column<OnChain>>> =
            versioned_storage.as_ref();

        let v1_fetched_after_taken =
            <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistoryV1<OnChain>,
            >>::get(as_structured_storage, &1)
            .unwrap();

        let v2_fetched_after_taken =
            <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistoryV2<OnChain>,
            >>::get(as_structured_storage, &1)
            .unwrap();

        // Then
        assert_eq!(taken, v2_value);
        assert!(v1_fetched_after_taken.is_none());
        assert!(v2_fetched_after_taken.is_none());
        assert!(fetched_after_taken.is_none());
    }

    #[test]
    fn take_returns_v1_on_old_values() {
        // Given
        let storage: InMemoryStorage<Column<OnChain>> = InMemoryStorage::default();
        let mut versioned_storage = VersionedStorage::new(storage);
        let as_structured_storage: &mut StructuredStorage<
            InMemoryStorage<Column<OnChain>>,
        > = versioned_storage.as_mut();

        let mut v1_value = Changes::default();
        with_empty_level(&mut v1_value, 42);

        // When

        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV1<OnChain>,
        >>::insert(as_structured_storage, &1, &v1_value)
        .unwrap();

        let taken =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
                ModificationsHistory<OnChain>,
            >>::take(&mut versioned_storage, &1)
            .unwrap()
            .unwrap();

        let fetched_after_taken =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistory<OnChain>,
            >>::get(&versioned_storage, &1)
            .unwrap();

        let as_structured_storage: &StructuredStorage<InMemoryStorage<Column<OnChain>>> =
            versioned_storage.as_ref();

        let v1_fetched_after_taken =
            <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistoryV1<OnChain>,
            >>::get(as_structured_storage, &1)
            .unwrap();

        // Then
        assert_eq!(taken, v1_value);
        assert!(v1_fetched_after_taken.is_none());
        assert!(fetched_after_taken.is_none());
    }

    #[test]
    fn replace_v1_with_v2() {
        // Given
        let storage: InMemoryStorage<Column<OnChain>> = InMemoryStorage::default();
        let mut versioned_storage = VersionedStorage::new(storage);
        let as_structured_storage: &mut StructuredStorage<
            InMemoryStorage<Column<OnChain>>,
        > = versioned_storage.as_mut();

        let v1_value = Changes::default();
        let mut v2_value = Changes::default();
        with_empty_level(&mut v2_value, 42);

        // When
        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV1<OnChain>,
        >>::insert(as_structured_storage, &1, &v1_value)
        .unwrap();

        let replaced =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
                ModificationsHistory<OnChain>,
            >>::replace(&mut versioned_storage, &1, &v2_value)
            .unwrap()
            .unwrap();

        let fetched_after_replaced =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistory<OnChain>,
            >>::get(&versioned_storage, &1)
            .unwrap()
            .unwrap()
            .into_owned();

        let as_structured_storage: &StructuredStorage<InMemoryStorage<Column<OnChain>>> =
            versioned_storage.as_ref();

        let v1_fetched_after_replaced =
            <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistoryV1<OnChain>,
            >>::get(as_structured_storage, &1)
            .unwrap()
            .map(|borrowed| borrowed.into_owned());

        let v2_fetched_after_replaced =
            <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistoryV1<OnChain>,
            >>::get(as_structured_storage, &1)
            .unwrap()
            .unwrap()
            .into_owned();
        // Then
        assert_eq!(replaced, v1_value);
        assert!(v1_fetched_after_replaced.is_none());
        assert_eq!(v2_fetched_after_replaced, v2_value);
        assert_eq!(fetched_after_replaced, v2_value);
    }

    #[test]
    fn replace_v2() {
        // Given
        let storage: InMemoryStorage<Column<OnChain>> = InMemoryStorage::default();
        let mut versioned_storage = VersionedStorage::new(storage);
        let as_structured_storage: &mut StructuredStorage<
            InMemoryStorage<Column<OnChain>>,
        > = versioned_storage.as_mut();

        let v2_old_value = Changes::default();
        let mut v2_new_value = Changes::default();
        with_empty_level(&mut v2_new_value, 42);

        // When
        <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
            ModificationsHistoryV2<OnChain>,
        >>::insert(as_structured_storage, &1, &v2_old_value)
        .unwrap();

        let replaced =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageMutate<
                ModificationsHistory<OnChain>,
            >>::replace(&mut versioned_storage, &1, &v2_new_value)
            .unwrap()
            .unwrap();

        let fetched_after_replaced =
            <VersionedStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistory<OnChain>,
            >>::get(&versioned_storage, &1)
            .unwrap()
            .unwrap()
            .into_owned();

        let as_structured_storage: &StructuredStorage<InMemoryStorage<Column<OnChain>>> =
            versioned_storage.as_ref();

        let v2_fetched_after_replaced =
            <StructuredStorage<InMemoryStorage<Column<OnChain>>> as StorageInspect<
                ModificationsHistoryV2<OnChain>,
            >>::get(as_structured_storage, &1)
            .unwrap()
            .unwrap()
            .into_owned();
        // Then
        assert_eq!(replaced, v2_old_value);
        assert_eq!(v2_fetched_after_replaced, v2_new_value);
        assert_eq!(fetched_after_replaced, v2_new_value);
    }
}
