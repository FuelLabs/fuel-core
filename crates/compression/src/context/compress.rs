use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        ContextError,
    },
    fuel_tx::{
        CompressedUtxoId,
        UtxoId,
    },
};

use crate::{
    compress::CompressDb,
    eviction_policy::CacheEvictor,
    tables::PerRegistryKeyspaceMap,
};

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
