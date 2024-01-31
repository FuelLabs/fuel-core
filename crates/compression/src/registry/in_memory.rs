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
    index: HashMap<&'static str, HashMap<Vec<u8>, RawKey>>,
}

impl RegistryDb for InMemoryRegistry {
    fn next_key<T: Table>(&self) -> Key<T> {
        Key::from_raw(self.next_keys.get(T::NAME).copied().unwrap_or(RawKey::ZERO))
    }

    fn read<T: Table>(&self, key: Key<T>) -> T::Type {
        if key == Key::DEFAULT_VALUE {
            return T::Type::default();
        }

        self.storage
            .get(T::NAME)
            .and_then(|table| table.get(&key.raw()))
            .map(|bytes| postcard::from_bytes(bytes).expect("Invalid value in registry"))
            .unwrap_or_default()
    }

    fn batch_write<T: Table>(&mut self, start_key: Key<T>, values: Vec<T::Type>) {
        let empty = values.is_empty();
        if !empty && start_key == Key::DEFAULT_VALUE {
            panic!("Cannot write to the default value key");
        }
        let table = self.storage.entry(T::NAME).or_default();
        let mut key = start_key.raw();
        for value in values.into_iter() {
            let value = postcard::to_stdvec(&value).unwrap();
            let mut prefix = value.clone();
            prefix.truncate(32);
            self.index.entry(T::NAME).or_default().insert(prefix, key);
            table.insert(key, value);
            key = key.next();
        }
        if !empty {
            self.next_keys.insert(T::NAME, key);
        }
    }

    fn index_lookup<T: Table>(&self, value: &T::Type) -> Option<Key<T>> {
        if *value == T::Type::default() {
            return Some(Key::DEFAULT_VALUE);
        }

        let needle = postcard::to_stdvec(value).unwrap();
        let mut prefix = needle.clone();
        prefix.truncate(32);
        if let Some(cand) = self.index.get(T::NAME)?.get(&prefix).copied() {
            let cand_val = self.storage.get(T::NAME)?.get(&cand)?;
            if *cand_val == needle {
                return Some(Key::from_raw(cand));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tables,
        Key,
    };

    #[test]
    fn in_memory_registry_works() {
        let mut reg = InMemoryRegistry::default();

        // Empty
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            [0; 32]
        );

        // Write
        reg.batch_write(
            Key::<tables::AssetId>::from_raw(RawKey::try_from(100u32).unwrap()),
            vec![[1; 32], [2; 32]],
        );

        // Read
        assert_eq!(
            reg.read(Key::<tables::AssetId>::try_from(100).unwrap()),
            [1; 32]
        );

        // Index
        assert_eq!(
            reg.index_lookup(&[1; 32]),
            Some(Key::<tables::AssetId>::try_from(100).unwrap())
        );
    }
}
