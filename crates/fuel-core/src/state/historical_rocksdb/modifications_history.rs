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
pub struct ModificationsHistoryVersion<Description, const N: usize>(
    core::marker::PhantomData<Description>,
)
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

/// Blueprint for Modificaations History V1. Keys are stored in little endian
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

// Default to V1 until we implement the versioned Storage.
// Switch to V2 once the implementation is done.
// Would be nice to have feature flags here.
type ModificationsHistoryV1<Description> = ModificationsHistoryVersion<Description, 0>;
type ModificationsHistoryV2<Description> = ModificationsHistoryVersion<Description, 1>;
/// The ModificationHistory type exposed to other modules.
pub type ModificationsHistory<Description> = ModificationsHistoryV2<Description>;

/// VersionedStorage<S> wraps a StructuredStorage and adds versioning capabilities for the
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
            ModificationsHistoryV1<Description>,
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
