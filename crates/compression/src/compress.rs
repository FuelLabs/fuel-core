use crate::{
    eviction_policy::CacheEvictor,
    ports::UtxoIdToPointer,
    registry::{
        PerRegistryKeyspace,
        RegistrationsPerTable,
        TemporalRegistryAll,
    },
    CompressedBlock,
    CompressedBlockPayloadV0,
};
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
};
use std::collections::{
    HashMap,
    HashSet,
};

pub trait CompressDb: TemporalRegistryAll + UtxoIdToPointer {}
impl<T> CompressDb for T where T: TemporalRegistryAll + UtxoIdToPointer {}

/// This must be called for all new blocks in sequence, otherwise the result will be garbage.
pub async fn compress<D>(mut db: D, block: &Block) -> anyhow::Result<Vec<u8>>
where
    D: CompressDb,
{
    let target = block.transactions_vec();

    let mut prepare_ctx = PrepareCtx::new(&mut db);
    let _ = target.compress_with(&mut prepare_ctx).await?;

    let mut ctx = prepare_ctx.into_compression_context();
    let transactions = target.compress_with(&mut ctx).await?;
    let registrations: RegistrationsPerTable = ctx.into();

    // Apply changes to the db
    registrations.write_to_registry(&mut db)?;

    // Construct the actual compacted block
    let compact = CompressedBlockPayloadV0 {
        registrations,
        registrations_root: Bytes32::default(), /* TODO: https://github.com/FuelLabs/fuel-core/issues/2232 */
        header: block.header().into(),
        transactions,
    };

    let compressed = postcard::to_allocvec(&CompressedBlock::V0(compact))
        .expect("Serialization cannot fail");

    Ok(compressed)
}

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

macro_rules! compression {
    ($($ident:ty: $type:ty),*) => { paste::paste! {
        pub struct CompressCtx<D> {
            db: D,
            $($ident: CompressCtxKeyspace<$type>,)*
        }

        impl<D> PrepareCtx<D> {
            /// Converts the preparation context into a [`CompressCtx`]
            /// keeping accessed keys to avoid its eviction during compression.
            pub fn into_compression_context(self) -> CompressCtx<D> {
                CompressCtx {
                    db: self.db,
                    $(
                        $ident: CompressCtxKeyspace {
                            changes: Default::default(),
                            cache_evictor: CacheEvictor::new(self.accessed_keys.$ident.into()),
                        },
                    )*
                }
            }
        }

        impl<D> From<CompressCtx<D>> for RegistrationsPerTable {
            fn from(value: CompressCtx<D>) -> Self {
                let mut result = Self::default();
                $(
                    for (key, value) in value.$ident.changes.into_iter() {
                        result.$ident.push((key, value));
                    }
                )*
                result
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

                    let key = ctx.$ident.cache_evictor.next_key(&mut ctx.db)?;
                    let old = ctx.$ident.changes.insert(key, self.clone());
                    assert!(old.is_none(), "Key collision in registry substitution");
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
