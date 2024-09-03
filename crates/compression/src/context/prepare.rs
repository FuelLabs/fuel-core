use std::collections::HashSet;

use anyhow::bail;
use fuel_core_types::{
    fuel_compression::{
        RawKey,
        RegistrySubstitutableBy,
    },
    fuel_tx::*,
};

use crate::{
    db::RocksDb,
    tables::{
        check_keyspace,
        PerRegistryKeyspace,
        RegistryKeyspace,
    },
};

/// Preparation pass through the block to collect all keys accessed during compression.
/// Returns placeholder. The resulting "compressed block" should be discarded.
pub struct PrepareCtx<'a> {
    /// Database handle
    pub db: &'a mut RocksDb,
    /// Keys accessed during compression. Will not be overwritten.
    pub accessed_keys: PerRegistryKeyspace<HashSet<RawKey>>,
}

fn registry_prepare<T: serde::Serialize + Default + PartialEq>(
    keyspace: RegistryKeyspace,
    value: &T,
    ctx: &mut PrepareCtx<'_>,
) -> anyhow::Result<RawKey> {
    if *value == T::default() {
        return Ok(RawKey::ZERO);
    }
    if let Some(found) = ctx.db.registry_index_lookup(keyspace, value)? {
        ctx.accessed_keys[keyspace].insert(found);
    }
    Ok(RawKey::ZERO)
}

impl RegistrySubstitutableBy<PrepareCtx<'_>> for Address {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut PrepareCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_prepare(
            check_keyspace!(keyspace, RegistryKeyspace::address),
            self,
            ctx,
        )
    }
}

impl RegistrySubstitutableBy<PrepareCtx<'_>> for AssetId {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut PrepareCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_prepare(
            check_keyspace!(keyspace, RegistryKeyspace::asset_id),
            self,
            ctx,
        )
    }
}

impl RegistrySubstitutableBy<PrepareCtx<'_>> for ContractId {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut PrepareCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_prepare(
            check_keyspace!(keyspace, RegistryKeyspace::contract_id),
            self,
            ctx,
        )
    }
}

impl RegistrySubstitutableBy<PrepareCtx<'_>> for Vec<u8> {
    fn substitute(
        &self,
        keyspace: &str,
        ctx: &mut PrepareCtx<'_>,
    ) -> anyhow::Result<RawKey> {
        registry_prepare(
            check_keyspace!(
                keyspace,
                RegistryKeyspace::script_code | RegistryKeyspace::witness
            ),
            self,
            ctx,
        )
    }
}
