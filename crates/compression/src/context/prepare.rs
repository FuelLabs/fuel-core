use std::collections::HashSet;

use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        RegistryKey,
    },
    fuel_tx::*,
};

use crate::{
    db::RocksDb,
    tables::{
        PerRegistryKeyspace,
        RegistryKeyspace,
    },
};

/// Preparation pass through the block to collect all keys accessed during compression.
/// Returns dummy values. The resulting "compressed block" should be discarded.
pub struct PrepareCtx<'a> {
    /// Database handle
    pub db: &'a mut RocksDb,
    /// Keys accessed during compression. Will not be overwritten.
    pub accessed_keys: PerRegistryKeyspace<HashSet<RegistryKey>>,
}

fn registry_prepare<T: serde::Serialize + Default + PartialEq>(
    keyspace: RegistryKeyspace,
    value: &T,
    ctx: &mut PrepareCtx<'_>,
) -> anyhow::Result<RegistryKey> {
    if *value == T::default() {
        return Ok(RegistryKey::ZERO);
    }
    if let Some(found) = ctx.db.registry_index_lookup(keyspace, value)? {
        ctx.accessed_keys[keyspace].insert(found);
    }
    Ok(RegistryKey::ZERO)
}

impl<'a> CompressibleBy<PrepareCtx<'a>, anyhow::Error> for Address {
    async fn compress(&self, ctx: &mut PrepareCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::address, self, ctx)
    }
}

impl<'a> CompressibleBy<PrepareCtx<'a>, anyhow::Error> for AssetId {
    async fn compress(&self, ctx: &mut PrepareCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::asset_id, self, ctx)
    }
}

impl<'a> CompressibleBy<PrepareCtx<'a>, anyhow::Error> for ContractId {
    async fn compress(&self, ctx: &mut PrepareCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::contract_id, self, ctx)
    }
}

impl<'a> CompressibleBy<PrepareCtx<'a>, anyhow::Error> for ScriptCode {
    async fn compress(&self, ctx: &mut PrepareCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_prepare(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<'a> CompressibleBy<PrepareCtx<'a>, anyhow::Error> for CompressibleTxId {
    async fn compress(&self, ctx: &mut PrepareCtx<'a>) -> anyhow::Result<TxPointer> {
        Ok(TxPointer::default())
    }
}
