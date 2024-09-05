use std::collections::HashMap;

use fuel_core_types::{
    fuel_compression::{
        CompressibleBy,
        RegistryKey,
    },
    fuel_tx::{
        Address,
        AssetId,
        ContractId,
        ScriptCode,
        TxPointer,
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

impl<'a> CompressibleBy<CompressCtx<'a>, anyhow::Error> for Address {
    async fn compress(&self, ctx: &mut CompressCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::address, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>, anyhow::Error> for AssetId {
    async fn compress(&self, ctx: &mut CompressCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::asset_id, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>, anyhow::Error> for ContractId {
    async fn compress(&self, ctx: &mut CompressCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::contract_id, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>, anyhow::Error> for ScriptCode {
    async fn compress(&self, ctx: &mut CompressCtx<'a>) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<'a> CompressibleBy<CompressCtx<'a>, anyhow::Error> for UtxoId {
    async fn compress(
        &self,
        ctx: &mut CompressCtx<'a>,
    ) -> anyhow::Result<(TxPointer, u16)> {
        ctx.tx_lookup.lookup(*self).await
    }
}
