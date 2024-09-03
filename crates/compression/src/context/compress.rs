use std::collections::HashMap;

use anyhow::bail;
use fuel_core_types::{
    fuel_compression::{
        RawKey,
        RegistrySubstitutableBy,
    },
    fuel_tx::{
        Address,
        AssetId,
        ContractId,
    },
};

use crate::{
    db::RocksDb,
    eviction_policy::CacheEvictor,
    tables::{
        check_keyspace,
        PerRegistryKeyspace,
        PostcardSerialized,
        RegistryKeyspace,
    },
};

pub struct CompressCtx<'a> {
    pub db: &'a mut RocksDb,
    pub cache_evictor: CacheEvictor,
    /// Changes to the temporary registry, to be included in the compressed block header
    pub changes: PerRegistryKeyspace<HashMap<RawKey, PostcardSerialized>>,
}

fn registry_substitute<T: serde::Serialize + Default + PartialEq>(
    keyspace: RegistryKeyspace,
    value: &T,
    ctx: &mut CompressCtx<'_>,
) -> anyhow::Result<RawKey> {
    if *value == T::default() {
        return Ok(RawKey::DEFAULT_VALUE);
    }

    if let Some(found) = ctx.db.registry_index_lookup(keyspace, value)? {
        return Ok(found);
    }

    let key = ctx.cache_evictor.next_key(keyspace)?;
    let old = ctx.changes[keyspace].insert(key, PostcardSerialized::new(value)?);
    assert!(old.is_none(), "Key collision in registry substitution");
    Ok(key)
}

impl RegistrySubstitutableBy<CompressCtx<'_>, anyhow::Error> for Address {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut CompressCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_substitute(
            check_keyspace!(keyspace, RegistryKeyspace::address),
            self,
            ctx,
        )
    }
}

impl RegistrySubstitutableBy<CompressCtx<'_>, anyhow::Error> for AssetId {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut CompressCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_substitute(
            check_keyspace!(keyspace, RegistryKeyspace::asset_id),
            self,
            ctx,
        )
    }
}

impl RegistrySubstitutableBy<CompressCtx<'_>, anyhow::Error> for ContractId {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut CompressCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_substitute(
            check_keyspace!(keyspace, RegistryKeyspace::contract_id),
            self,
            ctx,
        )
    }
}

impl RegistrySubstitutableBy<CompressCtx<'_>, anyhow::Error> for Vec<u8> {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut CompressCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_substitute(
            check_keyspace!(
                keyspace,
                RegistryKeyspace::script_code | RegistryKeyspace::witness
            ),
            self,
            ctx,
        )
    }
}
