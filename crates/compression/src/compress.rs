use crate::{
    config::Config,
    eviction_policy::CacheEvictor,
    ports::{
        EvictorDb,
        TemporalRegistry,
        UtxoIdToPointer,
    },
    registry::{
        PerRegistryKeyspace,
        RegistrationsPerTable,
        TemporalRegistryAll,
    },
    CompressedBlockPayloadV0,
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
        Bytes32,
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

pub trait CompressDb: TemporalRegistryAll + UtxoIdToPointer {}
impl<T> CompressDb for T where T: TemporalRegistryAll + UtxoIdToPointer {}

/// This must be called for all new blocks in sequence, otherwise the result will be garbage.
pub async fn compress<D>(
    config: Config,
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

    // Apply changes to the db
    registrations.write_to_registry(&mut db, block.header().consensus().time)?;

    // Construct the actual compacted block
    let compact = CompressedBlockPayloadV0 {
        registrations,
        registrations_root: Bytes32::default(), /* TODO: https://github.com/FuelLabs/fuel-core/issues/2232 */
        header: block.header().into(),
        transactions,
    };

    Ok(VersionedCompressedBlock::V0(compact))
}

/// Preparation pass through the block to collect all keys accessed during compression.
/// Returns dummy values. The resulting "compressed block" should be discarded.
pub struct PrepareCtx<D> {
    pub config: Config,
    /// Current timestamp
    pub timestamp: Tai64,
    /// Database handle
    pub db: D,
    /// Keys accessed during the compression.
    pub accessed_keys: PerRegistryKeyspace<HashSet<RegistryKey>>,
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
    /// Reverse lookup into changes
    changes_lookup: HashMap<T, RegistryKey>,
}

macro_rules! compression {
    ($($ident:ty: $type:ty),*) => { paste::paste! {
        pub struct CompressCtx<D> {
            config: Config,
            timestamp: Tai64,
            db: D,
            $($ident: CompressCtxKeyspace<$type>,)*
        }

        impl<D> PrepareCtx<D> where D: CompressDb {
            /// Converts the preparation context into a [`CompressCtx`]
            /// keeping accessed keys to avoid its eviction during compression.
            /// Initializes the cache evictors from the database, which may fail.
            pub fn into_compression_context(mut self) -> anyhow::Result<CompressCtx<D>> {
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

        impl<D> CompressCtx<D> where D: CompressDb {
            /// Finalizes the compression context, returning the changes to the registry.
            /// Commits the cache evictor states to the database.
            fn finalize(mut self) -> anyhow::Result<RegistrationsPerTable> {
                let mut registrations = RegistrationsPerTable::default();
                $(
                    self.$ident.cache_evictor.commit(&mut self.db)?;
                    for (key, value) in self.$ident.changes.into_iter() {
                        registrations.$ident.push((key, value));
                    }
                )*
                Ok(registrations)
            }
        }

        $(
            impl<D> CompressibleBy<PrepareCtx<D>> for $type
            where
                D: TemporalRegistry<$type> + EvictorDb<$type>
            {
                async fn compress_with(
                    &self,
                    ctx: &mut PrepareCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if *self == <$type>::default() {
                        return Ok(RegistryKey::ZERO);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        let key_timestamp = ctx.db.read_timestamp(&found)
                            .context("Database invariant violated: no timestamp stored but key found")?;
                        if ctx.config.is_timestamp_accessible(ctx.timestamp, key_timestamp)? {
                            ctx.accessed_keys.$ident.insert(found);
                        }
                    }
                    Ok(RegistryKey::ZERO)
                }
            }

            impl<D> CompressibleBy<CompressCtx<D>> for $type
            where
                D: TemporalRegistry<$type> + EvictorDb<$type>
            {
                async fn compress_with(
                    &self,
                    ctx: &mut CompressCtx<D>,
                ) -> anyhow::Result<RegistryKey> {
                    if self == &Default::default() {
                        return Ok(RegistryKey::DEFAULT_VALUE);
                    }
                    if let Some(found) = ctx.db.registry_index_lookup(self)? {
                        let key_timestamp = ctx.db.read_timestamp(&found)
                            .context("Database invariant violated: no timestamp stored but key found")?;
                        if ctx.config.is_timestamp_accessible(ctx.timestamp, key_timestamp)? {
                            return Ok(found);
                        }
                    }
                    if let Some(found) = ctx.$ident.changes_lookup.get(self) {
                        return Ok(*found);
                    }

                    let key = ctx.$ident.cache_evictor.next_key();
                    let old = ctx.$ident.changes.insert(key, self.clone());
                    let old_rev = ctx.$ident.changes_lookup.insert(self.clone(), key);
                    assert!(old.is_none(), "Key collision in registry substitution");
                    assert!(old_rev.is_none(), "Key collision in registry substitution");
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
