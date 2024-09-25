use std::collections::HashSet;

use crate::{
    eviction_policy::CacheEvictor,
    ports::{
        EvictorDb,
        UtxoIdToPointer,
    },
    tables::{
        PerRegistryKeyspace,
        PerRegistryKeyspaceMap,
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
        Bytes32,
        CompressedUtxoId,
        Transaction,
        TxPointer,
        UtxoId,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Only the next sequential block can be compressed")]
    NotLatest,
    #[error("Unknown compression error")]
    Other(#[from] anyhow::Error),
}

pub trait CompressDb: TemporalRegistryAll + UtxoIdToPointer {}
impl<T> CompressDb for T where T: TemporalRegistryAll + UtxoIdToPointer {}

/// This must be called for all new blocks in sequence, otherwise the result will be garbage.
pub async fn compress<D: CompressDb + EvictorDb>(
    db: D,
    block: &Block,
) -> Result<Vec<u8>, Error> {
    let target = block.transactions().to_vec();

    let mut prepare_ctx = PrepareCtx {
        db,
        accessed_keys: PerRegistryKeyspace::default(),
    };
    let _ =
        <Vec<Transaction> as CompressibleBy<_>>::compress_with(&target, &mut prepare_ctx)
            .await?;

    let mut ctx = CompressCtx {
        db: prepare_ctx.db,
        cache_evictor: CacheEvictor {
            keep_keys: prepare_ctx.accessed_keys,
        },
        changes: Default::default(),
    };
    let transactions = target.compress_with(&mut ctx).await?;
    let registrations = ctx.changes;
    let registrations = RegistrationsPerTable::try_from(registrations)?;

    // Apply changes to the db
    registrations.write_to_registry(&mut ctx.db)?;

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
    pub db: D,
    /// Keys accessed during compression. Will not be overwritten.
    pub accessed_keys: PerRegistryKeyspace<HashSet<RegistryKey>>,
}

impl<D> ContextError for PrepareCtx<D> {
    type Error = anyhow::Error;
}

impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for UtxoId {
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

pub struct CompressCtx<D> {
    pub db: D,
    pub cache_evictor: CacheEvictor,
    /// Changes to the temporary registry, to be included in the compressed block header
    pub changes: PerRegistryKeyspaceMap,
}

impl<D> ContextError for CompressCtx<D> {
    type Error = anyhow::Error;
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for UtxoId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<CompressedUtxoId> {
        ctx.db.lookup(*self)
    }
}
