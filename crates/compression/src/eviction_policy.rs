use std::collections::HashSet;

use fuel_core_types::fuel_compression::RegistryKey;

use crate::{
    ports::EvictorDb,
    tables::{
        PerRegistryKeyspace,
        RegistryKeyspace,
    },
};

pub struct CacheEvictor {
    /// Set of keys that must not be evicted
    pub keep_keys: PerRegistryKeyspace<HashSet<RegistryKey>>,
}

impl CacheEvictor {
    /// Get a key, evicting an old value if necessary
    pub fn next_key<D>(
        &mut self,
        db: &mut D,
        keyspace: RegistryKeyspace,
    ) -> anyhow::Result<RegistryKey>
    where
        D: EvictorDb,
    {
        // Pick first key not in the set
        // TODO: use a proper algo, maybe LRU?
        let mut key = db.read_latest(keyspace)?;

        debug_assert!(self.keep_keys[keyspace].len() < 2usize.pow(24).saturating_sub(2));

        while self.keep_keys[keyspace].contains(&key) {
            key = key.next();
        }

        db.write_latest(keyspace, key)?;

        self.keep_keys[keyspace].insert(key);
        Ok(key)
    }
}
