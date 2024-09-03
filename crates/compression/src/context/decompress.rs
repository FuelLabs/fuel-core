use anyhow::bail;
use fuel_core_types::{
    fuel_compression::{
        RawKey,
        RegistryDesubstitutableBy,
    },
    fuel_tx::{
        Address,
        AssetId,
        ContractId,
    },
};

use crate::{
    db::RocksDb,
    tables::{
        check_keyspace,
        RegistryKeyspace,
    },
};

pub struct DecompressCtx<'a> {
    pub db: &'a RocksDb,
}

impl<'a> RegistryDesubstitutableBy<DecompressCtx<'_>> for Address {
    fn desubstitute(
        c: &RawKey,
        ctx: &DecompressCtx,
        keyspace: &str,
    ) -> anyhow::Result<Self> {
        ctx.db
            .read_registry(check_keyspace!(keyspace, RegistryKeyspace::address), *c)
    }
}

impl<'a> RegistryDesubstitutableBy<DecompressCtx<'_>> for AssetId {
    fn desubstitute(
        c: &RawKey,
        ctx: &DecompressCtx,
        keyspace: &str,
    ) -> anyhow::Result<Self> {
        ctx.db
            .read_registry(check_keyspace!(keyspace, RegistryKeyspace::asset_id), *c)
    }
}

impl<'a> RegistryDesubstitutableBy<DecompressCtx<'_>> for ContractId {
    fn desubstitute(
        c: &RawKey,
        ctx: &DecompressCtx,
        keyspace: &str,
    ) -> anyhow::Result<Self> {
        ctx.db
            .read_registry(check_keyspace!(keyspace, RegistryKeyspace::contract_id), *c)
    }
}

impl<'a> RegistryDesubstitutableBy<DecompressCtx<'_>> for Vec<u8> {
    fn desubstitute(
        c: &RawKey,
        ctx: &DecompressCtx,
        keyspace: &str,
    ) -> anyhow::Result<Vec<u8>> {
        ctx.db.read_registry(
            check_keyspace!(
                keyspace,
                RegistryKeyspace::script_code | RegistryKeyspace::witness
            ),
            *c,
        )
    }
}
