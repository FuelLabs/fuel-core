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

impl<Description> TableWithBlueprint for ModificationsHistoryVersion<Description, 0>
where
    Description: DatabaseDescription,
{
    // TODO: The Blueprint should be `Plain<Primitive<8>, Postcard>` to sort
    //  the keys in the database. https://github.com/FuelLabs/fuel-core/issues/2095
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column<Description>;

    fn column() -> Self::Column {
        Column::HistoryColumn
    }
}

/// ModificationsHistoryV2 TableWithBlueprint definition
/// Keys are stored using Big Endian encoding
impl<Description> TableWithBlueprint for ModificationsHistoryVersion<Description, 1>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column<Description>;

    // We store key-value pairs using the same column family as for V1
    fn column() -> Self::Column {
        Column::HistoryColumn
    }
}

/// Default to V1 until we implement the versioned Storage.
/// Switch to V2 once the implementation is done.
/// Would be nice to have feature flags here.
type ModificationsHistoryV1<Description> = ModificationsHistoryVersion<Description, 0>;
type ModificationsHistoryV2<Description> = ModificationsHistoryVersion<Description, 1>;
pub type ModificationsHistory<Description> = ModificationsHistoryV1<Description>;

pub struct MigratedHistoryKeys<Description>(core::marker::PhantomData<Description>)
where
    Description: DatabaseDescription;

/// Migrated history .
impl<Description> Mappable for MigratedHistoryKeys<Description>
where
    Description: DatabaseDescription,
{
    /// The height of the modifications.
    type Key = u64;
    type OwnedKey = <ModificationsHistory<Description> as Mappable>::OwnedKey;
    /// Values are of unit type, as we are only
    /// interested in determining whether a key is present in the MigratedHistory table.
    type Value = ();
    type OwnedValue = Self::Value;
}

impl<Description> TableWithBlueprint for MigratedHistoryKeys<Description>
where
    Description: DatabaseDescription,
{
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column<Description>;

    fn column() -> Self::Column {
        Column::MigratedHistoryKeysColumn
    }
}

// We default to always writing using V1, e.g. the versioned storage
// behaves exactly as the structured storage.
// TODO: Implement versioned reads and writes with the following behaviour:
// - Writed always use the V2 encoding for keys
// - Reads try first to read using the V2 encoding of a key, if the key is not
//   found a subsequent read is tried using the V1 encoding.

pub struct VersionedStorage<S> {
    inner: StructuredStorage<S>,
}

impl<Description, S> StorageInspect<ModificationsHistory<Description>>
    for VersionedStorage<S>
where
    S: KeyValueInspect<Column = Column<Description>>,
    Description: DatabaseDescription,
{
    type Error = fuel_core_storage::Error;

    fn get(&self, key: &u64) -> Result<Option<Cow<Changes>>, Self::Error> {
        // Check first if key has been migrated
        let migrated = <StructuredStorage<S> as StorageInspect<
            MigratedHistoryKeys<Description>,
        >>::contains_key(&self.inner, key)?;
        if migrated {
            // If key has been migrated, read from ModificationsHistory using V2 encoding
            <StructuredStorage<S> as StorageInspect<
                ModificationsHistoryV2<Description>,
            >>::get(&self.inner, key)
        } else {
            // Otherwise read from ModificationsHistory using V1 encoding
            <StructuredStorage<S> as StorageInspect<
                ModificationsHistoryV1<Description>,
            >>::get(&self.inner, key)
        }
    }

    fn contains_key(&self, key: &u64) -> Result<bool, Self::Error> {
        let v2_result = <StructuredStorage<S> as StorageInspect<
            ModificationsHistoryVersion<Description, 0>,
        >>::contains_key(&self.inner, key)?;
        if v2_result {
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
        // first we can try to retrieve the value associated to the key, if any.
        // We default to the `StorageInspect` implementation for the versioned storage,
        // guaranteeing that we retrieve both migrated and non-migrated key-value pairs
        let old_value = self.get(key)?.map(|borrowed| borrowed.into_owned());

        // Write the new value using the V2 format
        <StructuredStorage<S> as StorageMutate<
                ModificationsHistoryV2<Description>,
            >>::replace(&mut self.inner, key, value)?;
        // Mark the key as migrated

        <StructuredStorage<S> as StorageMutate<
                MigratedHistoryKeys<Description>,
            >>::replace(&mut self.inner, key, &())?;
        // Remove the key-value pair where keys have been encoded using V1 of the encoding

        <StructuredStorage<S> as StorageMutate<
                ModificationsHistoryV1<Description>,
            >>::take(&mut self.inner, key)?;

        Ok(old_value)
    }

    fn take(&mut self, key: &u64) -> Result<Option<Changes>, Self::Error> {
        // Try to remove the key encoded with the V1 format,
        // If there is none this is essentially a no-op
        // If we find a value, we still try to remove the `MigratedHistoryKey` entry
        // and the key encoded with the V2 format, as we might
        // be in the middle of a migration.
        let v1_value_opt = <StructuredStorage<S> as StorageMutate<
            ModificationsHistoryV1<Description>,
        >>::take(&mut self.inner, key)?;
        // TODO: These two operations must be performed atomically to avoid a scenario where
        // we remove the MigratedHistoryKey from the storage, and then the system fails
        // before removing the key-value pair with the V2 encoding.
        <StructuredStorage<S> as StorageMutate<MigratedHistoryKeys<Description>>>::take(
            &mut self.inner,
            key,
        )?;
        let v2_value_opt = <StructuredStorage<S> as StorageMutate<
            ModificationsHistoryV2<Description>,
        >>::take(&mut self.inner, key)?;
        Ok(v2_value_opt.or(v1_value_opt))
    }
}
