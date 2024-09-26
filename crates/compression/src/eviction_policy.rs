use std::collections::HashSet;

use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        ContractId,
        ScriptCode,
    },
};

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

macro_rules! impl_evictor {
    ($type:ty) => { paste::paste! {
        impl CacheEvictor {
            /// Get a key, evicting an old value if necessary
            #[allow(non_snake_case)] // Match names of types exactly
            pub fn [< next_key_ $type >] <D>(
                &mut self,
                db: &mut D,
            ) -> anyhow::Result<RegistryKey>
            where
                D: EvictorDb<$type>,
            {
                // Pick first key not in the set
                // TODO: use a proper algo, maybe LRU?
                let mut key = db.read_latest()?;

                debug_assert!(self.keep_keys[RegistryKeyspace::[<$type>]].len() < 2usize.pow(24).saturating_sub(2));

                while self.keep_keys[RegistryKeyspace::[<$type>]].contains(&key) {
                    key = key.next();
                }

                db.write_latest(key)?;

                self.keep_keys[RegistryKeyspace::[<$type>]].insert(key);
                Ok(key)
            }
        }
    }};
}

impl_evictor!(Address);
impl_evictor!(AssetId);
impl_evictor!(ContractId);
impl_evictor!(ScriptCode);
impl_evictor!(PredicateCode);
