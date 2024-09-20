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
    compress::CompressDb,
    eviction_policy::CacheEvictor,
    tables::{
        PerRegistryKeyspace,
        PostcardSerialized,
        RegistryKeyspace,
    },
};

pub struct CompressCtx<D> {
    pub db: D,
    pub cache_evictor: CacheEvictor,
    /// Changes to the temporary registry, to be included in the compressed block header
    pub changes: PerRegistryKeyspace<HashMap<RegistryKey, PostcardSerialized>>,
}

impl<D> ContextError for CompressCtx<D> {
    type Error = anyhow::Error;
}

fn registry_substitute<D: CompressDb, T: serde::Serialize + Default + PartialEq>(
    keyspace: RegistryKeyspace,
    value: &T,
    ctx: &mut CompressCtx<D>,
) -> anyhow::Result<RegistryKey> {
    if *value == T::default() {
        return Ok(RegistryKey::DEFAULT_VALUE);
    }

    let ser_value = postcard::to_stdvec(value)?;
    if let Some(found) = ctx.db.registry_index_lookup(keyspace, ser_value)? {
        return Ok(found);
    }

    let key = ctx.cache_evictor.next_key(keyspace)?;
    let old = ctx.changes[keyspace].insert(key, PostcardSerialized::new(value)?);
    assert!(old.is_none(), "Key collision in registry substitution");
    Ok(key)
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for Address {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::address, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for AssetId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::asset_id, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for ContractId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::contract_id, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for ScriptCode {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for PredicateCode {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<RegistryKey> {
        registry_substitute(RegistryKeyspace::script_code, self, ctx)
    }
}

impl<D: CompressDb> CompressibleBy<CompressCtx<D>> for UtxoId {
    async fn compress_with(
        &self,
        ctx: &mut CompressCtx<D>,
    ) -> anyhow::Result<CompressedUtxoId> {
        ctx.db.lookup(*self)
    }
}
