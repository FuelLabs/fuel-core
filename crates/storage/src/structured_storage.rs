//! The module contains the [`StructuredStorage`] wrapper around the key-value storage
//! that implements the storage traits for the tables with blueprint.

use crate::{
    blueprint::{
        Blueprint,
        SupportsBatching,
    },
    column::Column,
    kv_store::{
        BatchOperations,
        KeyValueStore,
    },
    Error as StorageError,
    Mappable,
    StorageBatchMutate,
    StorageInspect,
    StorageMutate,
    StorageSize,
};
use std::borrow::Cow;

pub mod balances;
pub mod blocks;
pub mod coins;
pub mod contracts;
pub mod merkle_data;
pub mod messages;
pub mod receipts;
pub mod sealed_block;
pub mod state;
pub mod transactions;

/// The table can implement this trait to indicate that it has a blueprint.
/// It inherits the default implementation of the storage traits through the [`StructuredStorage`]
/// for the table.
pub trait TableWithBlueprint: Mappable + Sized {
    /// The type of the blueprint used by the table.
    type Blueprint;

    /// The column occupied by the table.
    fn column() -> Column;
}

/// The wrapper around the key-value storage that implements the storage traits for the tables
/// with blueprint.
#[derive(Clone, Debug)]
pub struct StructuredStorage<S> {
    pub(crate) storage: S,
}

impl<S> StructuredStorage<S> {
    /// Creates a new instance of the structured storage.
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S> AsRef<S> for StructuredStorage<S> {
    fn as_ref(&self) -> &S {
        &self.storage
    }
}

impl<S> AsMut<S> for StructuredStorage<S> {
    fn as_mut(&mut self) -> &mut S {
        &mut self.storage
    }
}

impl<S, M> StorageInspect<M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint,
    M::Blueprint: Blueprint<M, S>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::get(&self.storage, key, M::column())
            .map(|value| value.map(Cow::Owned))
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::exists(&self.storage, key, M::column())
    }
}

impl<S, M> StorageMutate<M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint,
    M::Blueprint: Blueprint<M, S>,
{
    fn insert(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::replace(
            &mut self.storage,
            key,
            M::column(),
            value,
        )
    }

    fn remove(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::take(&mut self.storage, key, M::column())
    }
}

impl<S, M> StorageSize<M> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
    M: Mappable + TableWithBlueprint,
    M::Blueprint: Blueprint<M, S>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        <M as TableWithBlueprint>::Blueprint::size_of_value(
            &self.storage,
            key,
            M::column(),
        )
    }
}

impl<S, M> StorageBatchMutate<M> for StructuredStorage<S>
where
    S: BatchOperations<Column = Column>,
    M: Mappable + TableWithBlueprint,
    M::Blueprint: SupportsBatching<M, S>,
{
    fn init_storage(
        &mut self,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> Result<(), Self::Error> {
        <M as TableWithBlueprint>::Blueprint::init(&mut self.storage, M::column(), set)
    }

    fn insert_batch(
        &mut self,
        set: &mut dyn Iterator<Item = (&M::Key, &M::Value)>,
    ) -> Result<(), Self::Error> {
        <M as TableWithBlueprint>::Blueprint::insert(&mut self.storage, M::column(), set)
    }

    fn remove_batch(
        &mut self,
        set: &mut dyn Iterator<Item = &M::Key>,
    ) -> Result<(), Self::Error> {
        <M as TableWithBlueprint>::Blueprint::remove(&mut self.storage, M::column(), set)
    }
}

/// The module that provides helper macros for testing the structured storage.
#[cfg(feature = "test-helpers")]
pub mod test {
    use crate as fuel_core_storage;
    use fuel_core_storage::{
        column::Column,
        kv_store::{
            BatchOperations,
            KeyValueStore,
            Value,
        },
        Result as StorageResult,
    };
    use std::{
        cell::RefCell,
        collections::HashMap,
    };

    type Storage = RefCell<HashMap<(Column, Vec<u8>), Vec<u8>>>;

    /// The in-memory storage for testing purposes.
    #[derive(Default, Debug, PartialEq, Eq)]
    pub struct InMemoryStorage {
        storage: Storage,
    }

    impl KeyValueStore for InMemoryStorage {
        type Column = Column;

        fn write(
            &self,
            key: &[u8],
            column: Self::Column,
            buf: &[u8],
        ) -> StorageResult<usize> {
            let write = buf.len();
            self.storage
                .borrow_mut()
                .insert((column, key.to_vec()), buf.to_vec());
            Ok(write)
        }

        fn delete(&self, key: &[u8], column: Self::Column) -> StorageResult<()> {
            self.storage.borrow_mut().remove(&(column, key.to_vec()));
            Ok(())
        }

        fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
            Ok(self
                .storage
                .borrow_mut()
                .get(&(column, key.to_vec()))
                .map(|v| v.clone().into()))
        }
    }

    impl BatchOperations for InMemoryStorage {}

    /// The macro that generates basic storage tests for the table with [`InMemoryStorage`].
    #[macro_export]
    macro_rules! basic_storage_tests {
        ($table:ident, $key:expr, $value_insert:expr, $value_return:expr, $random_key:expr) => {
            $crate::paste::item! {
            #[cfg(test)]
            #[allow(unused_imports)]
            mod [< $table:snake _basic_tests >] {
                use super::*;
                use $crate::{
                    structured_storage::{
                        test::InMemoryStorage,
                        StructuredStorage,
                    },
                    StorageAsMut,
                };
                use $crate::StorageInspect;
                use $crate::StorageMutate;
                use $crate::rand;

                #[allow(dead_code)]
                fn random<T, R>(rng: &mut R) -> T
                where
                    rand::distributions::Standard: rand::distributions::Distribution<T>,
                    R: rand::Rng,
                {
                    use rand::Rng;
                    rng.gen()
                }

                #[test]
                fn get() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);
                    let key = $key;

                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &$value_insert)
                        .unwrap();

                    assert_eq!(
                        structured_storage
                            .storage_as_mut::<$table>()
                            .get(&key)
                            .expect("Should get without errors")
                            .expect("Should not be empty")
                            .into_owned(),
                        $value_return
                    );
                }

                #[test]
                fn insert() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);
                    let key = $key;

                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &$value_insert)
                        .unwrap();

                    let returned = structured_storage
                        .storage_as_mut::<$table>()
                        .get(&key)
                        .unwrap()
                        .unwrap()
                        .into_owned();
                    assert_eq!(returned, $value_return);
                }

                #[test]
                fn remove() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);
                    let key = $key;

                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &$value_insert)
                        .unwrap();

                    structured_storage.storage_as_mut::<$table>().remove(&key).unwrap();

                    assert!(!structured_storage
                        .storage_as_mut::<$table>()
                        .contains_key(&key)
                        .unwrap());
                }

                #[test]
                fn exists() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);
                    let key = $key;

                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &$value_insert)
                        .unwrap();

                    assert!(structured_storage
                        .storage_as_mut::<$table>()
                        .contains_key(&key)
                        .unwrap());
                }

                #[test]
                fn batch_mutate_works() {
                    use $crate::rand::{
                        Rng,
                        rngs::StdRng,
                        RngCore,
                        SeedableRng,
                    };

                    let empty_storage = InMemoryStorage::default();

                    let mut init_storage = InMemoryStorage::default();
                    let mut init_structured_storage = StructuredStorage::new(&mut init_storage);

                    let mut rng = &mut StdRng::seed_from_u64(1234);
                    let gen = || Some($random_key(&mut rng));
                    let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();
                    let value = $value_insert;

                    <_ as $crate::StorageBatchMutate<$table>>::init_storage(
                        &mut init_structured_storage,
                        &mut data.iter().map(|k| {
                            let value: &<$table as $crate::Mappable>::Value = &value;
                            (k, value)
                        })
                    ).expect("Should initialize the storage successfully");

                    let mut insert_storage = InMemoryStorage::default();
                    let mut insert_structured_storage = StructuredStorage::new(&mut insert_storage);

                    <_ as $crate::StorageBatchMutate<$table>>::insert_batch(
                        &mut insert_structured_storage,
                        &mut data.iter().map(|k| {
                            let value: &<$table as $crate::Mappable>::Value = &value;
                            (k, value)
                        })
                    ).expect("Should insert batch successfully");

                    assert_eq!(init_storage, insert_storage);
                    assert_ne!(init_storage, empty_storage);
                    assert_ne!(insert_storage, empty_storage);

                    let mut remove_from_insert_structured_storage = StructuredStorage::new(&mut insert_storage);
                    <_ as $crate::StorageBatchMutate<$table>>::remove_batch(
                        &mut remove_from_insert_structured_storage,
                        &mut data.iter()
                    ).expect("Should remove all entries successfully from insert storage");
                    assert_ne!(init_storage, insert_storage);
                    assert_eq!(insert_storage, empty_storage);

                    let mut remove_from_init_structured_storage = StructuredStorage::new(&mut init_storage);
                    <_ as $crate::StorageBatchMutate<$table>>::remove_batch(
                        &mut remove_from_init_structured_storage,
                        &mut data.iter()
                    ).expect("Should remove all entries successfully from init storage");
                    assert_eq!(init_storage, insert_storage);
                    assert_eq!(init_storage, empty_storage);
                }
            }}
        };
        ($table:ident, $key:expr, $value_insert:expr, $value_return:expr) => {
            $crate::basic_storage_tests!($table, $key, $value_insert, $value_return, random);
        };
        ($table:ident, $key:expr, $value:expr) => {
            $crate::basic_storage_tests!($table, $key, $value, $value);
        };
    }

    /// The macro that generates SMT storage tests for the table with [`InMemoryStorage`].
    #[macro_export]
    macro_rules! root_storage_tests {
        ($table:ident, $metadata_table:ident, $current_key:expr, $foreign_key:expr, $generate_key:ident, $generate_value:ident) => {
            paste::item! {
            #[cfg(test)]
            mod [< $table:snake _root_tests >] {
                use super::*;
                use $crate::{
                    structured_storage::{
                        test::InMemoryStorage,
                        StructuredStorage,
                    },
                    StorageAsMut,
                };
                use $crate::rand::{
                    rngs::StdRng,
                    SeedableRng,
                };

                #[test]
                fn root() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let rng = &mut StdRng::seed_from_u64(1234);
                    let key = $generate_key(&$current_key, rng);
                    let value = $generate_value(rng);
                    structured_storage.storage_as_mut::<$table>().insert(&key, &value)
                        .unwrap();

                    let root = structured_storage.storage_as_mut::<$table>().root(&$current_key);
                    assert!(root.is_ok())
                }

                #[test]
                fn root_returns_empty_root_for_empty_metadata() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let empty_root = fuel_core_types::fuel_merkle::sparse::in_memory::MerkleTree::new().root();
                    let root = structured_storage
                        .storage_as_mut::<$table>()
                        .root(&$current_key)
                        .unwrap();
                    assert_eq!(root, empty_root)
                }

                #[test]
                fn put_updates_the_state_merkle_root_for_the_given_metadata() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let rng = &mut StdRng::seed_from_u64(1234);
                    let key = $generate_key(&$current_key, rng);
                    let state = $generate_value(rng);

                    // Write the first contract state
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &state)
                        .unwrap();

                    // Read the first Merkle root
                    let root_1 = structured_storage
                        .storage_as_mut::<$table>()
                        .root(&$current_key)
                        .unwrap();

                    // Write the second contract state
                    let key = $generate_key(&$current_key, rng);
                    let state = $generate_value(rng);
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &state)
                        .unwrap();

                    // Read the second Merkle root
                    let root_2 = structured_storage
                        .storage_as_mut::<$table>()
                        .root(&$current_key)
                        .unwrap();

                    assert_ne!(root_1, root_2);
                }

                #[test]
                fn remove_updates_the_state_merkle_root_for_the_given_metadata() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let rng = &mut StdRng::seed_from_u64(1234);

                    // Write the first contract state
                    let first_key = $generate_key(&$current_key, rng);
                    let first_state = $generate_value(rng);
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&first_key, &first_state)
                        .unwrap();
                    let root_0 = structured_storage
                        .storage_as_mut::<$table>()
                        .root(&$current_key)
                        .unwrap();

                    // Write the second contract state
                    let second_key = $generate_key(&$current_key, rng);
                    let second_state = $generate_value(rng);
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&second_key, &second_state)
                        .unwrap();

                    // Read the first Merkle root
                    let root_1 = structured_storage
                        .storage_as_mut::<$table>()
                        .root(&$current_key)
                        .unwrap();

                    // Remove the second contract state
                    structured_storage.storage_as_mut::<$table>().remove(&second_key).unwrap();

                    // Read the second Merkle root
                    let root_2 = structured_storage
                        .storage_as_mut::<$table>()
                        .root(&$current_key)
                        .unwrap();

                    assert_ne!(root_1, root_2);
                    assert_eq!(root_0, root_2);
                }

                #[test]
                fn updating_foreign_metadata_does_not_affect_the_given_metadata_insertion() {
                    let given_primary_key = $current_key;
                    let foreign_primary_key = $foreign_key;

                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let rng = &mut StdRng::seed_from_u64(1234);

                    let state_value = $generate_value(rng);

                    // Given
                    let given_key = $generate_key(&given_primary_key, rng);
                    let foreign_key = $generate_key(&foreign_primary_key, rng);
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&given_key, &state_value)
                        .unwrap();

                    // When
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&foreign_key, &state_value)
                        .unwrap();
                    structured_storage
                        .storage_as_mut::<$table>()
                        .remove(&foreign_key)
                        .unwrap();

                    // Then
                    let result = structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&given_key, &state_value)
                        .unwrap();

                    assert!(result.is_some());
                }

                #[test]
                fn put_creates_merkle_metadata_when_empty() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let rng = &mut StdRng::seed_from_u64(1234);

                    // Given
                    let key = $generate_key(&$current_key, rng);
                    let state = $generate_value(rng);

                    // Write a contract state
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &state)
                        .unwrap();

                    // Read the Merkle metadata
                    let metadata = structured_storage
                        .storage_as_mut::<$metadata_table>()
                        .get(&$current_key)
                        .unwrap();

                    assert!(metadata.is_some());
                }

                #[test]
                fn remove_deletes_merkle_metadata_when_empty() {
                    let mut storage = InMemoryStorage::default();
                    let mut structured_storage = StructuredStorage::new(&mut storage);

                    let rng = &mut StdRng::seed_from_u64(1234);

                    // Given
                    let key = $generate_key(&$current_key, rng);
                    let state = $generate_value(rng);

                    // Write a contract state
                    structured_storage
                        .storage_as_mut::<$table>()
                        .insert(&key, &state)
                        .unwrap();

                    // Read the Merkle metadata
                    structured_storage
                        .storage_as_mut::<$metadata_table>()
                        .get(&$current_key)
                        .unwrap()
                        .expect("Expected Merkle metadata to be present");

                    // Remove the contract asset
                    structured_storage.storage_as_mut::<$table>().remove(&key).unwrap();

                    // Read the Merkle metadata
                    let metadata = structured_storage
                        .storage_as_mut::<$metadata_table>()
                        .get(&$current_key)
                        .unwrap();

                    assert!(metadata.is_none());
                }
            }}
        };
    }
}
