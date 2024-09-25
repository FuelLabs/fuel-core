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
    compress::CompressDb,
    context::{
        compress::CompressCtx,
        decompress::DecompressCtx,
        prepare::PrepareCtx,
    },
    decompress::{
        DecompressDb,
        DecompressError,
    },
    ports::{
        EvictorDb,
        TemporalRegistry,
    },
};

macro_rules! tables {
    ($($name:ident: $type:ty),*$(,)?) => {
        #[doc = "RegistryKey namespaces"]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[allow(non_camel_case_types)] // Match names in structs exactly
        #[derive(strum::EnumDiscriminants)]
        #[strum_discriminants(name(RegistryKeyspace))]
        #[strum_discriminants(derive(Hash, serde::Serialize, serde::Deserialize))]
        #[strum_discriminants(allow(non_camel_case_types))]
        pub enum RegistryKeyspaceValue {
            $(
                $name($type),
            )*
        }


        impl RegistryKeyspaceValue {
            pub fn keyspace(&self) -> RegistryKeyspace {
                match self {
                    $(
                        RegistryKeyspaceValue::$name(_) => RegistryKeyspace::$name,
                    )*
                }
            }
        }


        #[doc = "A value for each keyspace"]
        #[derive(Debug, Clone, Default)]
        pub struct PerRegistryKeyspace<T> {
            $(pub $name: T,)*
        }
        impl<T> core::ops::Index<RegistryKeyspace> for PerRegistryKeyspace<T> {
            type Output = T;

            fn index(&self, index: RegistryKeyspace) -> &Self::Output {
                match index {
                    $(
                        RegistryKeyspace::$name => &self.$name,
                    )*
                }
            }
        }
        impl<T> core::ops::IndexMut<RegistryKeyspace> for PerRegistryKeyspace<T> {
            fn index_mut(&mut self, index: RegistryKeyspace) -> &mut Self::Output {
                match index {
                    $(
                        RegistryKeyspace::$name => &mut self.$name,
                    )*
                }
            }
        }

        #[doc = "Key-value mapping for each keyspace"]
        #[derive(Debug, Clone, Default)]
        pub struct PerRegistryKeyspaceMap {
            $(pub $name: HashMap<RegistryKey, $type>,)*
        }

        impl PerRegistryKeyspaceMap {
            #[cfg(test)]
            pub fn insert(&mut self, key: RegistryKey, value: RegistryKeyspaceValue) {
                match value {
                    $(
                        RegistryKeyspaceValue::$name(value) => {
                            self.$name.insert(key, value);
                        }
                    )*
                }
            }
        }

        #[doc = "The set of registrations for each table, as used in the compressed block header"]
        #[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
        pub struct RegistrationsPerTable {
            $(pub $name: Vec<(RegistryKey, $type)>,)*
        }

        impl TryFrom<PerRegistryKeyspaceMap> for RegistrationsPerTable {
            type Error = anyhow::Error;

            fn try_from(value: PerRegistryKeyspaceMap) -> Result<Self, Self::Error> {
                let mut result = Self::default();
                $(
                    for (key, value) in value.$name.into_iter() {
                        result.$name.push((key, value));
                    }
                )*
                Ok(result)
            }
        }

        impl RegistrationsPerTable {
            pub(crate) fn write_to_registry<R: TemporalRegistry>(&self, registry: &mut R) -> anyhow::Result<()> {
                $(
                    for (key, value) in self.$name.iter() {
                        registry.write_registry(*key, RegistryKeyspaceValue::$name(value.clone()))?;
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
                    if let Some(found) = ctx.db.registry_index_lookup(RegistryKeyspaceValue::$name(self.clone()))? {
                        ctx.accessed_keys[RegistryKeyspace::$name].insert(found);
                    }
                    Ok(RegistryKey::ZERO)
                }
            }

            impl<D: CompressDb + EvictorDb> CompressibleBy<CompressCtx<D>> for $type {
                async fn compress_with(
                    &self,
                    ctx: &mut CompressCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if *self == Default::default() {
                        return Ok(RegistryKey::DEFAULT_VALUE);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(RegistryKeyspaceValue::$name(self.clone()))? {
                        return Ok(found);
                    }

                    let key = ctx.cache_evictor.next_key(&mut ctx.db, RegistryKeyspace::$name)?;
                    let old = ctx.changes.$name.insert(key, self.clone());
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
                    match ctx.db.read_registry(RegistryKeyspace::$name, key)? {
                        RegistryKeyspaceValue::$name(value) => Ok(value),
                        _ => panic!("Registry returned incorrectly-typed value")
                    }
                }
            }
        )*
    };
}

tables!(
    address: Address,
    asset_id: AssetId,
    contract_id: ContractId,
    script_code: ScriptCode,
    predicate_code: PredicateCode,
);

// TODO: move inside the macro when this stabilizes: https://github.com/rust-lang/rust/pull/122808
#[cfg(any(test, feature = "test-helpers"))]
impl rand::prelude::Distribution<RegistryKeyspace> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> RegistryKeyspace {
        match rng.gen_range(0..5) {
            0 => RegistryKeyspace::address,
            1 => RegistryKeyspace::asset_id,
            2 => RegistryKeyspace::contract_id,
            3 => RegistryKeyspace::script_code,
            _ => RegistryKeyspace::predicate_code,
        }
    }
}
