use std::collections::HashSet;

use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        ContextError,
        RegistryKey,
    },
    fuel_tx::*,
};
use input::PredicateCode;

use crate::{
    services::compress::CompressDb,
    tables::{
        PerRegistryKeyspace,
        RegistryKeyspace,
    },
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

fn registry_prepare<D: CompressDb, T: serde::Serialize + Default + PartialEq>(
    keyspace: RegistryKeyspace,
    value: &T,
    ctx: &mut PrepareCtx<D>,
) -> anyhow::Result<RegistryKey> {
    if *value == T::default() {
        return Ok(RegistryKey::ZERO);
    }
    let value = postcard::to_stdvec(value)?;
    if let Some(found) = ctx.db.registry_index_lookup(keyspace, value)? {
        ctx.accessed_keys[keyspace].insert(found);
    }
    Ok(RegistryKey::ZERO)
}

impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for Address {
    async fn compress_with(
        &self,
        ctx: &mut PrepareCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::address, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for AssetId {
    async fn compress_with(
        &self,
        ctx: &mut PrepareCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::asset_id, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for ContractId {
    async fn compress_with(
        &self,
        ctx: &mut PrepareCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::contract_id, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for ScriptCode {
    async fn compress_with(
        &self,
        ctx: &mut PrepareCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<PrepareCtx<D>> for PredicateCode {
    async fn compress_with(
        &self,
        ctx: &mut PrepareCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::script_code, self, ctx)
    }
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
