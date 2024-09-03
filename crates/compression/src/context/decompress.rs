use fuel_core_types::{
    fuel_compression::{
        RawKey,
        RegistryDesubstitutableBy,
    },
    fuel_tx::{
        Address,
        AssetId,
        ContractId,
        ScriptCode,
    },
};

use crate::{
    db::RocksDb,
    tables::RegistryKeyspace,
};

pub struct DecompressCtx<'a> {
    pub db: &'a RocksDb,
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for Address {
    fn desubstitute(c: &RawKey, ctx: &DecompressCtx) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::address, *c)
    }
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for AssetId {
    fn desubstitute(c: &RawKey, ctx: &DecompressCtx) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::asset_id, *c)
    }
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for ContractId {
    fn desubstitute(c: &RawKey, ctx: &DecompressCtx) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::contract_id, *c)
    }
}

impl RegistryDesubstitutableBy<DecompressCtx<'_>, anyhow::Error> for ScriptCode {
    fn desubstitute(c: &RawKey, ctx: &DecompressCtx) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::script_code, *c)
    }
}
