use std::collections::HashMap;

use super::{
    db::*,
    key::RawKey,
    Key,
    Table,
};

/// Simple and inefficient in-memory registry for testing purposes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InMemoryRegistry {
    next_keys: HashMap<&'static str, RawKey>,
    storage: HashMap<&'static str, HashMap<RawKey, Vec<u8>>>,
}

impl RegistrySelectNextKey for InMemoryRegistry {
    fn next_key<T: Table>(&mut self) -> Key<T> {
        let next_key = self.next_keys.entry(T::NAME).or_default();
        let key = Key::<T>::from_raw(*next_key);
        *next_key = next_key.next();
        key
    }
}

impl RegistryRead for InMemoryRegistry {
    fn read<T: Table>(&self, key: Key<T>) -> T::Type {
        self.storage
            .get(T::NAME)
            .and_then(|table| table.get(&key.raw()))
            .map(|bytes| postcard::from_bytes(bytes).expect("Invalid value in registry"))
            .unwrap_or_default()
    }
}

impl RegistryWrite for InMemoryRegistry {
    fn batch_write<T: Table>(&mut self, start_key: Key<T>, values: Vec<T::Type>) {
        let table = self.storage.entry(T::NAME).or_default();
        let mut key = start_key.raw();
        for value in values.into_iter() {
            table.insert(key, postcard::to_stdvec(&value).unwrap());
            key = key.next();
        }
    }
}

impl RegistryIndex for InMemoryRegistry {
    fn index_lookup<T: Table>(&self, value: &T::Type) -> Option<Key<T>> {
        let needle = postcard::to_stdvec(value).unwrap();
        let table = self.storage.get(T::NAME)?;
        for (key, value) in table.iter() {
            if value == &needle {
                return Some(Key::from_raw(*key));
            }
        }
        None
    }
}
