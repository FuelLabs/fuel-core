use std::collections::HashSet;

use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        ContextError,
        RegistryKey,
    },
    fuel_tx::*,
};

use crate::{
    compress::CompressDb,
    tables::PerRegistryKeyspace,
};

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
