use crate::{
    config::Config,
    eviction_policy::CacheEvictor,
    ports::{
        EvictorDb,
        TemporalRegistry,
        UtxoIdToPointer,
    },
    registry::{
        EvictorDbAll,
        PerRegistryKeyspace,
        RegistrationsPerTable,
        TemporalRegistryAll,
    },
    VersionedCompressedBlock,
};
use anyhow::Context;
use fuel_core_types::{
    blockchain::block::Block,
    fuel_compression::{
        CompressibleBy,
        ContextError,
        RegistryKey,
    },
    fuel_tx::{
        input::PredicateCode,
        CompressedUtxoId,
        ScriptCode,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        ContractId,
    },
    tai64::Tai64,
};
use std::collections::{
    HashMap,
    HashSet,
};

#[cfg(not(feature = "fault-proving"))]
pub mod not_fault_proving {
    use super::*;
    pub trait CompressDb: TemporalRegistryAll + EvictorDbAll + UtxoIdToPointer {}
    impl<T> CompressDb for T where T: TemporalRegistryAll + EvictorDbAll + UtxoIdToPointer {}
}

#[cfg(feature = "fault-proving")]
pub mod fault_proving {
    use super::*;
    use crate::ports::GetRegistryRoot;
    pub trait CompressDb:
        TemporalRegistryAll + EvictorDbAll + UtxoIdToPointer + GetRegistryRoot
    {
    }
    impl<T> CompressDb for T where
        T: TemporalRegistryAll + EvictorDbAll + UtxoIdToPointer + GetRegistryRoot
    {
    }
}

#[cfg(feature = "fault-proving")]
use fault_proving::CompressDb;

#[cfg(not(feature = "fault-proving"))]
use not_fault_proving::CompressDb;

/// This must be called for all new blocks in sequence, otherwise the result will be garbage, since
/// the registry is valid for only the current block height. On any other height you could be
/// referring to keys that have already been overwritten, or have not been written to yet.
pub async fn compress<D>(
    config: &'_ Config,
    mut db: D,
    block: &Block,
) -> anyhow::Result<VersionedCompressedBlock>
where
    D: CompressDb,
{
    let target = block.transactions_vec();

    let mut prepare_ctx = PrepareCtx {
        config,
        timestamp: block.header().time(),
        db: &mut db,
        accessed_keys: Default::default(),
    };
    let _ = target.compress_with(&mut prepare_ctx).await?;

    let mut ctx = prepare_ctx.into_compression_context()?;
    let transactions = target.compress_with(&mut ctx).await?;
    let registrations: RegistrationsPerTable = ctx.finalize()?;

    #[cfg(feature = "fault-proving")]
    let registry_root = db
        .registry_root()
        .map_err(|e| anyhow::anyhow!("Failed to get registry root: {}", e))?;

    Ok(VersionedCompressedBlock::new(
        block.header(),
        registrations,
        transactions,
        #[cfg(feature = "fault-proving")]
        registry_root,
    ))
}

/// Preparation pass through the block to collect all keys accessed during compression.
/// Returns dummy values. The resulting "compressed block" should be discarded.
struct PrepareCtx<'a, D> {
    config: &'a Config,
    /// Current timestamp
    timestamp: Tai64,
    /// Database handle
    db: D,
    /// Keys accessed during the compression.
    accessed_keys: PerRegistryKeyspace<HashSet<RegistryKey>>,
}

impl<'a, D> ContextError for PrepareCtx<'a, D> {
    type Error = anyhow::Error;
}

impl<'a, D> CompressibleBy<PrepareCtx<'a, D>> for UtxoId
where
    D: CompressDb,
{
    async fn compress_with(
        &self,
        _ctx: &mut PrepareCtx<'a, D>,
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
    /// Reverse lookup into changes
    changes_lookup: HashMap<T, RegistryKey>,
}

macro_rules! compression {
    ($($ident:ty: $type:ty),*) => { paste::paste! {
        pub struct CompressCtx<'a, D> {
            config: &'a Config,
            timestamp: Tai64,
            db: D,
            $($ident: CompressCtxKeyspace<$type>,)*
        }

        impl<'a, D> PrepareCtx<'a, D> where D: CompressDb {
            /// Converts the preparation context into a [`CompressCtx`]
            /// keeping accessed keys to avoid its eviction during compression.
            /// Initializes the cache evictors from the database, which may fail.
            pub fn into_compression_context(mut self) -> anyhow::Result<CompressCtx<'a, D>> {
                Ok(CompressCtx {
                    $(
                        $ident: CompressCtxKeyspace {
                            changes: Default::default(),
                            changes_lookup: Default::default(),
                            cache_evictor: CacheEvictor::new_from_db(&mut self.db, self.accessed_keys.$ident.into())?,
                        },
                    )*
                    config: self.config,
                    timestamp: self.timestamp,
                    db: self.db,
                })
            }
        }

        impl<'a, D> CompressCtx<'a, D> where D: CompressDb {
            /// Finalizes the compression context, returning the changes to the registry.
            /// Commits the registrations and cache evictor states to the database.
            fn finalize(mut self) -> anyhow::Result<RegistrationsPerTable> {
                let mut registrations = RegistrationsPerTable::default();
                $(
                    self.$ident.cache_evictor.commit(&mut self.db)?;
                    for (key, value) in self.$ident.changes.into_iter() {
                        registrations.$ident.push((key, value));
                    }
                )*
                registrations.write_to_registry(&mut self.db, self.timestamp)?;
                Ok(registrations)
            }
        }

        $(
            impl<'a, D> CompressibleBy<PrepareCtx<'a, D>> for $type
            where
                D: TemporalRegistry<$type> + EvictorDb<$type>
            {
                async fn compress_with(
                    &self,
                    ctx: &mut PrepareCtx<'a, D>,
                ) -> anyhow::Result<RegistryKey> {
                    if *self == <$type>::default() {
                        return Ok(RegistryKey::ZERO);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        if !ctx.accessed_keys.$ident.contains(&found) {
                            let key_timestamp = ctx.db.read_timestamp(&found)
                                .context("Database invariant violated: no timestamp stored but key found")?;
                            if ctx.config.is_timestamp_accessible(ctx.timestamp, key_timestamp)? {
                                ctx.accessed_keys.$ident.insert(found);
                            }
                        }
                    }
                    Ok(RegistryKey::ZERO)
                }
            }

            impl<'a, D> CompressibleBy<CompressCtx<'a, D>> for $type
            where
                D: TemporalRegistry<$type> + EvictorDb<$type>
            {
                async fn compress_with(
                    &self,
                    ctx: &mut CompressCtx<'a, D>,
                ) -> anyhow::Result<RegistryKey> {
                    if self == &Default::default() {
                        return Ok(RegistryKey::DEFAULT_VALUE);
                    }
                    if let Some(found) = ctx.$ident.changes_lookup.get(self) {
                        return Ok(*found);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        let key_timestamp = ctx.db.read_timestamp(&found)
                            .context("Database invariant violated: no timestamp stored but key found")?;
                        if ctx.config.is_timestamp_accessible(ctx.timestamp, key_timestamp)? {
                            return Ok(found);
                        }
                    }

                    let key = ctx.$ident.cache_evictor.next_key();
                    let old = ctx.$ident.changes.insert(key, self.clone());
                    let old_rev = ctx.$ident.changes_lookup.insert(self.clone(), key);
                    debug_assert!(old.is_none(), "Key collision in registry substitution");
                    debug_assert!(old_rev.is_none(), "Key collision in registry substitution");
                    Ok(key)
                }
            }
        )*
    }};
}

compression!(
    address: Address,
    asset_id: AssetId,
    contract_id: ContractId,
    script_code: ScriptCode,
    predicate_code: PredicateCode
);

impl<'a, D> ContextError for CompressCtx<'a, D> {
    type Error = anyhow::Error;
}

impl<'a, D> CompressibleBy<CompressCtx<'a, D>> for UtxoId
where
    D: CompressDb,
{
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a, D>,
    ) -> anyhow::Result<CompressedUtxoId> {
        ctx.db.lookup(*self)
    }
}
