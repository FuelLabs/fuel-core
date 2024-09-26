use std::collections::HashSet;

use fuel_core_types::fuel_compression::RegistryKey;

use crate::ports::EvictorDb;

/// Evictor for a single keyspace
#[derive(Debug)]
pub(crate) struct CacheEvictor<T> {
    pub keyspace: std::marker::PhantomData<T>,
    /// Set of keys that must not be evicted
    pub keep_keys: HashSet<RegistryKey>,
}

impl<T> CacheEvictor<T> {
    /// Get a key, evicting an old value if necessary
    #[allow(non_snake_case)] // Match names of types exactly
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
