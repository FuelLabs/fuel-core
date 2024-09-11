use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_tx::{
        Address,
        AssetId,
        ContractId,
    },
};
use std::collections::HashMap;

use crate::ports::TemporalRegistry;

/// Type-erased (serialized) data
#[derive(Debug, Clone)]
pub struct PostcardSerialized(Vec<u8>);
impl PostcardSerialized {
    pub(crate) fn new<T: serde::Serialize>(value: T) -> anyhow::Result<Self> {
        Ok(Self(postcard::to_stdvec(&value)?))
    }
}

macro_rules! tables {
    ($($name:ident: $type:ty),*$(,)?) => {
        #[doc = "RegistryKey namespaces"]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[allow(non_camel_case_types)] // Match names in structs exactly
        pub enum RegistryKeyspace {
            $(
                $name,
            )*
        }
        impl RegistryKeyspace {
            pub fn name(&self) -> &'static str {
                match self {
                    $(
                        Self::$name => stringify!($name),
                    )*
                }
            }
            pub fn from_str(name: &str) -> Option<Self> {
                match name {
                    $(
                        stringify!($name) => Some(Self::$name),
                    )*
                    _ => None,
                }
            }
        }

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

        #[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
        pub struct RegistrationsPerTable {
            $(pub $name: Vec<(RegistryKey, $type)>,)*
        }

        impl TryFrom<PerRegistryKeyspace<HashMap<RegistryKey, PostcardSerialized>>> for RegistrationsPerTable {
            type Error = anyhow::Error;

            fn try_from(value: PerRegistryKeyspace<HashMap<RegistryKey, PostcardSerialized>>) -> Result<Self, Self::Error> {
                let mut result = Self::default();
                $(
                    for (key, value) in value.$name.into_iter() {
                        result.$name.push((key, postcard::from_bytes(&value.0)?));
                    }
                )*
                Ok(result)
            }
        }

        impl RegistrationsPerTable {
            #[cfg(test)]
            pub(crate) fn is_empty(&self) -> bool {
                $(
                    if !self.$name.is_empty() {
                        return false;
                    }
                )*
                true
            }

            pub(crate) fn write_to_registry<R: TemporalRegistry>(&self, registry: &mut R) -> anyhow::Result<()> {
                $(
                    for (key, value) in self.$name.iter() {
                        registry.write_registry(RegistryKeyspace::$name, *key, postcard::to_stdvec(value)?)?;
                    }
                )*

                Ok(())
            }
        }
    };
}

tables!(
    address: Address,
    asset_id: AssetId,
    contract_id: ContractId,
    script_code: Vec<u8>,
    predicate_code: Vec<u8>,
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
