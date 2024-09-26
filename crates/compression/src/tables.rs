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
use std::collections::HashMap;

use crate::{
    compress::{
        CompressCtx,
        CompressDb,
        PrepareCtx,
    },
    decompress::{
        DecompressCtx,
        DecompressDb,
        DecompressError,
    },
    ports::{
        EvictorDb,
        TemporalRegistry,
    },
};

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

        #[doc = "Key-value mapping for each keyspace"]
        #[derive(Debug, Clone, Default)]
        #[allow(non_snake_case)] // Match type names exactly
        pub struct PerRegistryKeyspaceMap {
            $(pub [<$type>]: HashMap<RegistryKey, $type>,)*
        }

        #[doc = "The set of registrations for each table, as used in the compressed block header"]
        #[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
        #[allow(non_snake_case)] // Match type names exactly
        pub struct RegistrationsPerTable {
            $(pub [<$type>]: Vec<(RegistryKey, $type)>,)*
        }

        impl TryFrom<PerRegistryKeyspaceMap> for RegistrationsPerTable {
            type Error = anyhow::Error;

            fn try_from(value: PerRegistryKeyspaceMap) -> Result<Self, Self::Error> {
                let mut result = Self::default();
                $(
                    for (key, value) in value.[<$type>].into_iter() {
                        result.[<$type>].push((key, value));
                    }
                )*
                Ok(result)
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

                    let key = ctx.cache_evictor.[< next_key_ $type >](&mut ctx.db)?;
                    let old = ctx.changes.[<$type>].insert(key, self.clone());
                    assert!(old.is_none(), "Key collision in registry substitution");
                    Ok(key)
                }
            }

            impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for $type {
                async fn decompress_with(
                    key: RegistryKey,
                    ctx: &DecompressCtx<D>,
                ) -> Result<Self, DecompressError> {
                    if key == RegistryKey::DEFAULT_VALUE {
                        return Ok(<$type>::default());
                    }
                    Ok(ctx.db.read_registry(key)?)
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
