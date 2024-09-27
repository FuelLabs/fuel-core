use std::collections::HashSet;

use fuel_core_types::fuel_compression::RegistryKey;

use crate::ports::EvictorDb;

/// Evictor for a single keyspace
#[derive(Debug)]
#[must_use = "Evictor must be committed to the database to persist state"]
pub(crate) struct CacheEvictor<T> {
    /// Set of keys that must not be evicted
    keep_keys: HashSet<RegistryKey>,
    /// Next key to be used
    next_key: RegistryKey,
    /// Marker for the keyspace type
    _keyspace_marker: std::marker::PhantomData<T>,
}

impl<T> CacheEvictor<T> {
    /// Create new evictor, reading state from the database
    pub fn new_from_db<D>(
        db: &mut D,
        keep_keys: HashSet<RegistryKey>,
    ) -> anyhow::Result<Self>
    where
        D: EvictorDb<T>,
    {
        Ok(Self {
            keep_keys,
            next_key: db.get_latest_assigned_key()?,
            _keyspace_marker: std::marker::PhantomData,
        })
    }

    pub fn next_key(&mut self) -> RegistryKey {
        // Pick first key not in the set
        // TODO: use a proper algo, maybe LRU?

        debug_assert!(self.keep_keys.len() < 2usize.pow(24).saturating_sub(2));

        while self.keep_keys.contains(&self.next_key) {
            self.next_key = self.next_key.next();
        }

        self.keep_keys.insert(self.next_key);
        self.next_key
    }

    /// Commit the current state of the evictor to the database
    pub fn commit<D>(self, db: &mut D) -> anyhow::Result<()>
    where
        D: EvictorDb<T>,
    {
        db.set_latest_assigned_key(self.next_key)
    }
}
