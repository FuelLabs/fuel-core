use std::collections::HashSet;

use fuel_core_types::fuel_compression::RegistryKey;

use crate::{
    ports::EvictorDb,
    tables::{
        PerRegistryKeyspace,
        RegistryKeyspace,
    },
};

#[derive(Default)]
pub struct CacheEvictor {
    /// Set of keys that are already overwritten by this evictor
    consumed: PerRegistryKeyspace<HashSet<RegistryKey>>,
    /// Set of keys that must not be evicted, as they are dependencies of the current block
    reserved: PerRegistryKeyspace<HashSet<RegistryKey>>,
}

impl CacheEvictor {
    /// Attempts mark key as non-evictable for this block.
    /// Returns false if the key has already been consumed.
    #[must_use]
    pub fn try_reserve(&mut self, keyspace: RegistryKeyspace, key: &RegistryKey) -> bool {
        if self.consumed[keyspace].contains(key) {
            false
        } else {
            self.reserved[keyspace].insert(*key);
            true
        }
    }

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

        debug_assert!(
            self.consumed[keyspace]
                .len()
                .saturating_add(self.reserved[keyspace].len())
                < 2usize.pow(24).saturating_sub(2)
        );

        while self.consumed[keyspace].contains(&key)
            || self.reserved[keyspace].contains(&key)
        {
            key = key.next();
        }

        db.write_latest(keyspace, key)?;

        self.consumed[keyspace].insert(key);
        Ok(key)
    }
}
