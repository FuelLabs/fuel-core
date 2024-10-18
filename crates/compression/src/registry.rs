use crate::ports::{
    EvictorDb,
    TemporalRegistry,
};
use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        ContractId,
        ScriptCode,
    },
    tai64::Tai64,
};

macro_rules! tables {
    ($($ident:ty: $type:ty),*) => { paste::paste! {
        #[doc = "RegistryKey namespaces"]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, strum_macros::EnumCount)]
        pub enum RegistryKeyspace {
            $(
                [<$type>],
            )*
        }

        #[doc = "A value for each keyspace"]
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct PerRegistryKeyspace<T> {
            $(pub $ident: T,)*
        }
        impl<T> core::ops::Index<RegistryKeyspace> for PerRegistryKeyspace<T> {
            type Output = T;

            fn index(&self, index: RegistryKeyspace) -> &Self::Output {
                match index {
                    $(
                        RegistryKeyspace::[<$type>] => &self.$ident,
                    )*
                }
            }
        }
        impl<T> core::ops::IndexMut<RegistryKeyspace> for PerRegistryKeyspace<T> {
            fn index_mut(&mut self, index: RegistryKeyspace) -> &mut Self::Output {
                match index {
                    $(
                        RegistryKeyspace::[<$type>] => &mut self.$ident,
                    )*
                }
            }
        }

        #[doc = "The set of registrations for each table, as used in the compressed block header"]
        #[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
        pub struct RegistrationsPerTable {
            $(pub $ident: Vec<(RegistryKey, $type)>,)*
        }

        pub trait TemporalRegistryAll
        where
            $(Self: TemporalRegistry<$type>,)*
        {}

        impl<T> TemporalRegistryAll for T
        where
            $(T: TemporalRegistry<$type>,)*
        {}

        pub trait EvictorDbAll
        where
            $(Self: EvictorDb<$type>,)*
        {}

        impl<T> EvictorDbAll for T
        where
            $(T: EvictorDb<$type>,)*
        {}


        impl RegistrationsPerTable {
            pub fn write_to_registry<R>(&self, registry: &mut R, timestamp: Tai64) -> anyhow::Result<()>
            where
                R: TemporalRegistryAll
            {
                $(
                    for (key, value) in self.$ident.iter() {
                        registry.write_registry(key, value, timestamp)?;
                    }
                )*

                Ok(())
            }
        }
    }};
}

tables!(
    address: Address,
    asset_id: AssetId,
    contract_id: ContractId,
    script_code: ScriptCode,
    predicate_code: PredicateCode
);

// TODO: move inside the macro when this stabilizes: https://github.com/rust-lang/rust/pull/122808
#[cfg(any(test, feature = "test-helpers"))]
impl rand::prelude::Distribution<RegistryKeyspace> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> RegistryKeyspace {
        use strum::EnumCount;
        match rng.gen_range(0..RegistryKeyspace::COUNT) {
            0 => RegistryKeyspace::Address,
            1 => RegistryKeyspace::AssetId,
            2 => RegistryKeyspace::ContractId,
            3 => RegistryKeyspace::ScriptCode,
            4 => RegistryKeyspace::PredicateCode,
            _ => unreachable!("New keyspace is added but not supported here"),
        }
    }
}
