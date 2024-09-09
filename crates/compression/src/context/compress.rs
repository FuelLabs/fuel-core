use std::collections::HashMap;

use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        ContextError,
        RegistryKey,
    },
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        CompressedUtxoId,
        ContractId,
        ScriptCode,
        UtxoId,
    },
};

use crate::{
    db::RocksDb,
    eviction_policy::CacheEvictor,
    ports::UtxoIdToPointer,
    tables::{
        PerRegistryKeyspace,
        PostcardSerialized,
        RegistryKeyspace,
    },
};

pub struct CompressCtx<'a> {
    pub db: &'a mut RocksDb,
    pub tx_lookup: &'a dyn UtxoIdToPointer,
    pub cache_evictor: CacheEvictor,
    /// Changes to the temporary registry, to be included in the compressed block header
    pub changes: PerRegistryKeyspace<HashMap<RegistryKey, PostcardSerialized>>,
}

impl ContextError for CompressCtx<'_> {
    type Error = anyhow::Error;
}

fn registry_substitute<T: serde::Serialize + Default + PartialEq>(
    keyspace: RegistryKeyspace,
    value: &T,
    ctx: &mut CompressCtx<'_>,
) -> anyhow::Result<RegistryKey> {
    if *value == T::default() {
        return Ok(RegistryKey::DEFAULT_VALUE);
    }

    if let Some(found) = ctx.db.registry_index_lookup(keyspace, value)? {
        return Ok(found);
    }

    let key = ctx.cache_evictor.next_key(keyspace)?;
    let old = ctx.changes[keyspace].insert(key, PostcardSerialized::new(value)?);
    assert!(old.is_none(), "Key collision in registry substitution");
    Ok(key)
}

impl<'a> CompressibleBy<CompressCtx<'a>> for Address {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::address, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>> for AssetId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::asset_id, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>> for ContractId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::contract_id, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>> for ScriptCode {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>> for PredicateCode {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>> for UtxoId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<CompressedUtxoId> {
        ctx.tx_lookup.lookup(*self).await
    }
}
