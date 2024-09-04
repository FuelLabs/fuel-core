use fuel_core_types::{
    fuel_compression::{
        DecompressibleBy,
        RegistryKey,
    },
    fuel_tx::{
        Address,
        AssetId,
        CompressibleTxId,
        ContractId,
        ScriptCode,
        TxPointer,
    },
};

use crate::{
    db::RocksDb,
    ports::TxPointerToId,
    tables::RegistryKeyspace,
};

pub struct DecompressCtx<'a> {
    pub db: &'a RocksDb,
    pub tx_lookup: &'a dyn TxPointerToId,
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for Address {
    async fn decompress(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::address, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for AssetId {
    async fn decompress(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::asset_id, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for ContractId {
    async fn decompress(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::contract_id, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for ScriptCode {
    async fn decompress(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::script_code, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for CompressibleTxId {
    async fn decompress(c: &TxPointer, ctx: &DecompressCtx<'a>) -> anyhow::Result<Self> {
        Ok(ctx.tx_lookup.lookup(*c).await?.into())
    }
}
