use std::collections::HashSet;

use fuel_core_types::fuel_compression::RegistryKey;

use crate::ports::EvictorDb;

/// Evictor for a single keyspace
#[derive(Debug)]
pub(crate) struct CacheEvictor<T> {
    /// Set of keys that must not be evicted
    keep_keys: HashSet<RegistryKey>,
    _keyspace_marker: std::marker::PhantomData<T>,
}

impl<T> CacheEvictor<T> {
    pub fn new(keep_keys: HashSet<RegistryKey>) -> Self {
        Self {
            keep_keys,
            _keyspace_marker: std::marker::PhantomData,
        }
    }

    pub fn next_key<D>(&mut self, db: &mut D) -> anyhow::Result<RegistryKey>
    where
        D: EvictorDb<T>,
    {
        // Pick first key not in the set
        // TODO: use a proper algo, maybe LRU?
        let mut key = db.read_latest()?;

        debug_assert!(self.keep_keys.len() < 2usize.pow(24).saturating_sub(2));

        while self.keep_keys.contains(&key) {
            key = key.next();
        }

        db.write_latest(key)?;

        self.keep_keys.insert(key);
        Ok(key)
    }
}
