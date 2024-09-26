use crate::{
    compress::{
        CompressCtx,
        CompressDb,
        PrepareCtx,
    },
    decompress::{
        DecompressCtx,
        DecompressDb,
    },
    ports::{
        EvictorDb,
        TemporalRegistry,
    },
};
use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        DecompressibleBy,
        RegistryKey,
    },
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        ContractId,
        ScriptCode,
    },
};
use std::collections::HashSet;

macro_rules! tables {
    ($($type:ty),*$(,)?) => { paste::paste! {
        #[doc = "RegistryKey namespaces"]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum RegistryKeyspace {
            $(
                [<$type>],
            )*
        }

        #[doc = "A value for each keyspace"]
        #[derive(Debug, Clone, Default)]
        #[allow(non_snake_case)] // Match type names exactly
        pub struct PerRegistryKeyspace<T> {
            $(pub [<$type>]: T,)*
        }
        impl<T> core::ops::Index<RegistryKeyspace> for PerRegistryKeyspace<T> {
            type Output = T;

            fn index(&self, index: RegistryKeyspace) -> &Self::Output {
                match index {
                    $(
                        RegistryKeyspace::[<$type>] => &self.[<$type>],
                    )*
                }
            }
        }
        impl<T> core::ops::IndexMut<RegistryKeyspace> for PerRegistryKeyspace<T> {
            fn index_mut(&mut self, index: RegistryKeyspace) -> &mut Self::Output {
                match index {
                    $(
                        RegistryKeyspace::[<$type>] => &mut self.[<$type>],
                    )*
                }
            }
        }

        // If Rust had HKTs, we wouldn't have to do this
        #[derive(Debug)]
        #[allow(non_snake_case)] // Match type names exactly
        pub(crate) struct CompressCtxKeyspaces {
            $(pub [<$type>]: crate::compress::CompressCtxKeyspace<$type>,)*
        }

        impl From<PerRegistryKeyspace<HashSet<RegistryKey>>> for CompressCtxKeyspaces {
            fn from(value: PerRegistryKeyspace<HashSet<RegistryKey>>) -> Self {
                Self {
                    $(
                        [<$type>]: crate::compress::CompressCtxKeyspace {
                            changes: Default::default(),
                            cache_evictor: crate::eviction_policy::CacheEvictor {
                                keyspace: std::marker::PhantomData,
                                keep_keys: value.[<$type>],
                            },
                        },
                    )*
                }
            }
        }

        #[doc = "The set of registrations for each table, as used in the compressed block header"]
        #[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // Match type names exactly
        pub struct RegistrationsPerTable {
            $(pub [<$type>]: Vec<(RegistryKey, $type)>,)*
        }

        impl From<CompressCtxKeyspaces> for RegistrationsPerTable {
            fn from(value: CompressCtxKeyspaces) -> Self {
                let mut result = Self::default();
                $(
                    for (key, value) in value.[<$type>].changes.into_iter() {
                        result.[<$type>].push((key, value));
                    }
                )*
                result
            }
        }

        pub trait TemporalRegistryAll: Sized $(
            + TemporalRegistry<$type>
            + EvictorDb<$type>
        )* {}

        impl<T> TemporalRegistryAll for T where T: Sized $(
            + TemporalRegistry<$type>
            + EvictorDb<$type>
        )* {}


        impl RegistrationsPerTable {
            pub(crate) fn write_to_registry<R: TemporalRegistryAll>(&self, registry: &mut R) -> anyhow::Result<()> {
                $(
                    for (key, value) in self.[<$type>].iter() {
                        registry.write_registry(*key, value)?;
                    }
                )*

                Ok(())
            }
        }

        $(
            impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for $type {
                async fn compress_with(
                    &self,
                    ctx: &mut PrepareCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if *self == <$type>::default() {
                        return Ok(RegistryKey::ZERO);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        ctx.accessed_keys[RegistryKeyspace::[<$type>]].insert(found);
                    }
                    Ok(RegistryKey::ZERO)
                }
            }

            impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for $type {
                async fn compress_with(
                    &self,
                    ctx: &mut CompressCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if *self == Default::default() {
                        return Ok(RegistryKey::DEFAULT_VALUE);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        return Ok(found);
                    }

                    let key = ctx.per_keyspace.[<$type>].cache_evictor.next_key(&mut ctx.db)?;
                    let old = ctx.per_keyspace.[<$type>].changes.insert(key, self.clone());
                    assert!(old.is_none(), "Key collision in registry substitution");
                    Ok(key)
                }
            }

            impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for $type {
                async fn decompress_with(
                    key: RegistryKey,
                    ctx: &DecompressCtx<D>,
                ) -> anyhow::Result<Self> {
                    if key == RegistryKey::DEFAULT_VALUE {
                        return Ok(<$type>::default());
                    }
                    ctx.db.read_registry(key)
                }
            }
        )*
    }};
}

tables!(Address, AssetId, ContractId, ScriptCode, PredicateCode,);

// TODO: move inside the macro when this stabilizes: https://github.com/rust-lang/rust/pull/122808
#[cfg(any(test, feature = "test-helpers"))]
impl rand::prelude::Distribution<RegistryKeyspace> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> RegistryKeyspace {
        match rng.gen_range(0..5) {
            0 => RegistryKeyspace::Address,
            1 => RegistryKeyspace::AssetId,
            2 => RegistryKeyspace::ContractId,
            3 => RegistryKeyspace::ScriptCode,
            _ => RegistryKeyspace::PredicateCode,
        }
    }
}
