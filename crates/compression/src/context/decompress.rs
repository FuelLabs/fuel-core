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

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for Address {
    fn desubstitute(
        c: &RawKey,
        keyspace: &str,
        ctx: &DecompressCtx,
    ) -> anyhow::Result<Self> {
        ctx.db
            .read_registry(check_keyspace!(keyspace, RegistryKeyspace::address), *c)
    }
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for AssetId {
    fn desubstitute(
        c: &RawKey,
        keyspace: &str,
        ctx: &DecompressCtx,
    ) -> anyhow::Result<Self> {
        ctx.db
            .read_registry(check_keyspace!(keyspace, RegistryKeyspace::asset_id), *c)
    }
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for ContractId {
    fn desubstitute(
        c: &RawKey,
        keyspace: &str,
        ctx: &DecompressCtx,
    ) -> anyhow::Result<Self> {
        ctx.db
            .read_registry(check_keyspace!(keyspace, RegistryKeyspace::contract_id), *c)
    }
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for Vec<u8> {
    fn desubstitute(
        c: &RawKey,
        keyspace: &str,
        ctx: &DecompressCtx,
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
