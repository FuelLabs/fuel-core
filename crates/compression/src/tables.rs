use crate::{
    compress::CompressDb,
    decompress::{
        DecompressCtx,
        DecompressDb,
    },
    eviction_policy::CacheEvictor,
    ports::{
        EvictorDb,
        TemporalRegistry,
    },
};
use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        ContextError,
        DecompressibleBy,
        RegistryKey,
    },
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        CompressedUtxoId,
        ContractId,
        ScriptCode,
        TxPointer,
        UtxoId,
    },
};
use std::collections::{
    HashMap,
    HashSet,
};

/// Preparation pass through the block to collect all keys accessed during compression.
/// Returns dummy values. The resulting "compressed block" should be discarded.
pub struct PrepareCtx<D> {
    /// Database handle
    db: D,
    /// Keys accessed during the compression.
    accessed_keys: PerRegistryKeyspace<HashSet<RegistryKey>>,
}

impl<D> PrepareCtx<D> {
    /// Create a new PrepareCtx around the given database.
    pub fn new(db: D) -> Self {
        Self {
            db,
            accessed_keys: PerRegistryKeyspace::default(),
        }
    }

    /// Converts the preparation context into a [`CompressCtx`]
    /// keeping accessed keys to avoid its eviction during compression.
    pub fn into_compression_context(self) -> CompressCtx<D> {
        CompressCtx {
            db: self.db,
            per_keyspace: self.accessed_keys.into(),
        }
    }
}

impl<D> ContextError for PrepareCtx<D> {
    type Error = anyhow::Error;
}

impl<D> CompressibleBy<PrepareCtx<D>> for UtxoId
where
    D: CompressDb,
{
    async fn compress_with(
        &self,
        _ctx: &mut PrepareCtx<D>,
    ) -> anyhow::Result<CompressedUtxoId> {
        Ok(CompressedUtxoId {
            tx_pointer: TxPointer::default(),
            output_index: 0,
        })
    }
}

#[derive(Debug)]
struct CompressCtxKeyspace<T> {
    /// Cache evictor state for this keyspace
    cache_evictor: CacheEvictor<T>,
    /// Changes to the temporary registry, to be included in the compressed block header
    changes: HashMap<RegistryKey, T>,
}

pub struct CompressCtx<D> {
    db: D,
    per_keyspace: CompressCtxKeyspaces,
}

impl<D> CompressCtx<D> {
    /// Converts the compression context into a [`RegistrationsPerTable`]
    pub fn into_registrations(self) -> RegistrationsPerTable {
        self.per_keyspace.into()
    }
}

impl<D> ContextError for CompressCtx<D> {
    type Error = anyhow::Error;
}

impl<D> CompressibleBy<CompressCtx<D>> for UtxoId
where
    D: CompressDb,
{
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<CompressedUtxoId> {
        ctx.db.lookup(*self)
    }
}

macro_rules! tables {
    ($($ident:ty: $type:ty),*) => { paste::paste! {
        #[doc = "RegistryKey namespaces"]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum RegistryKeyspace {
            $(
                [<$type>],
            )*
        }

        #[doc = "A value for each keyspace"]
        #[derive(Debug, Clone, Default)]
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

        // If Rust had HKTs, we wouldn't have to do this
        #[derive(Debug)]
        struct CompressCtxKeyspaces {
            $(pub $ident: CompressCtxKeyspace<$type>,)*
        }

        impl From<PerRegistryKeyspace<HashSet<RegistryKey>>> for CompressCtxKeyspaces {
            fn from(value: PerRegistryKeyspace<HashSet<RegistryKey>>) -> Self {
                Self {
                    $(
                        $ident: CompressCtxKeyspace {
                            changes: Default::default(),
                            cache_evictor: CacheEvictor::new(value.$ident),
                        },
                    )*
                }
            }
        }

        #[doc = "The set of registrations for each table, as used in the compressed block header"]
        #[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
        pub struct RegistrationsPerTable {
            $(pub $ident: Vec<(RegistryKey, $type)>,)*
        }

        impl From<CompressCtxKeyspaces> for RegistrationsPerTable {
            fn from(value: CompressCtxKeyspaces) -> Self {
                let mut result = Self::default();
                $(
                    for (key, value) in value.$ident.changes.into_iter() {
                        result.$ident.push((key, value));
                    }
                )*
                result
            }
        }

        pub trait TemporalRegistryAll
        where
            Self: Sized,
            $(Self: TemporalRegistry<$type> + EvictorDb<$type>,)*
        {}

        impl<T> TemporalRegistryAll for T
        where
            T: Sized,
            $(T: TemporalRegistry<$type> + EvictorDb<$type>,)*
        {}


        impl RegistrationsPerTable {
            pub(crate) fn write_to_registry<R>(&self, registry: &mut R) -> anyhow::Result<()>
            where
                R: TemporalRegistryAll
            {
                $(
                    for (key, value) in self.$ident.iter() {
                        registry.write_registry(key, value)?;
                    }
                )*

                Ok(())
            }
        }

        $(
            impl<D> CompressibleBy<PrepareCtx<D>> for $type
            where
                D: CompressDb
            {
                async fn compress_with(
                    &self,
                    ctx: &mut PrepareCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if *self == <$type>::default() {
                        return Ok(RegistryKey::ZERO);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        ctx.accessed_keys.$ident.insert(found);
                    }
                    Ok(RegistryKey::ZERO)
                }
            }

            impl<D> CompressibleBy<CompressCtx<D>> for $type
            where
                D: CompressDb
            {
                async fn compress_with(
                    &self,
                    ctx: &mut CompressCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if self == &Default::default() {
                        return Ok(RegistryKey::DEFAULT_VALUE);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        return Ok(found);
                    }

                    let key = ctx.per_keyspace.$ident.cache_evictor.next_key(&mut ctx.db)?;
                    let old = ctx.per_keyspace.$ident.changes.insert(key, self.clone());
                    assert!(old.is_none(), "Key collision in registry substitution");
                    Ok(key)
                }
            }

            impl<D> DecompressibleBy<DecompressCtx<D>> for $type
            where
                D: DecompressDb
            {
                async fn decompress_with(
                    key: RegistryKey,
                    ctx: &DecompressCtx<D>,
                ) -> anyhow::Result<Self> {
                    if key == RegistryKey::DEFAULT_VALUE {
                        return Ok(<$type>::default());
                    }
                    ctx.db.read_registry(&key)
                }
            }
        )*
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
        match rng.gen_range(0..5) {
            0 => RegistryKeyspace::Address,
            1 => RegistryKeyspace::AssetId,
            2 => RegistryKeyspace::ContractId,
            3 => RegistryKeyspace::ScriptCode,
            _ => RegistryKeyspace::PredicateCode,
        }
    }
}
