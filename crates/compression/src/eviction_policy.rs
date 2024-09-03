use std::collections::HashSet;

use fuel_core_types::fuel_compression::RawKey;

use crate::tables::{
    PerRegistryKeyspace,
    RegistryKeyspace,
};

pub struct CacheEvictor {
    /// Set of keys that must not be evicted
    pub keep_keys: PerRegistryKeyspace<HashSet<RawKey>>,
}

impl CacheEvictor {
    /// Get a key, evicting an old value if necessary
    pub fn next_key(&mut self, keyspace: RegistryKeyspace) -> anyhow::Result<RawKey> {
        // Pick first key not in the set
        // TODO: this can be optimized by keeping a counter of the last key used
        // TODO: use a proper algo, maybe LRU?
        let mut key = RawKey::ZERO;
        while self.keep_keys[keyspace].contains(&key) {
            key = key.next();
            assert_ne!(key, RawKey::ZERO, "Ran out of keys");
        }

        self.keep_keys[keyspace].insert(key);
        Ok(key)
    }
}
