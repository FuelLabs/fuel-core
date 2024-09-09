use fuel_core_types::{
    fuel_compression::{
        Compressible,
        ContextError,
        Decompress,
        DecompressibleBy,
        RegistryKey,
    },
    fuel_tx::{
        input::{
            coin::{
                Coin,
                CoinSpecification,
            },
            message::{
                Message,
                MessageSpecification,
            },
            AsField,
            PredicateCode,
        },
        Address,
        AssetId,
        CompressedUtxoId,
        ContractId,
        Mint,
        ScriptCode,
        Transaction,
        UtxoId,
    },
};

use crate::{
    db::RocksDb,
    ports::HistoryLookup,
    tables::RegistryKeyspace,
};

pub struct DecompressCtx<'a> {
    pub db: &'a RocksDb,
    pub lookup: &'a dyn HistoryLookup,
}

impl<'a> ContextError for DecompressCtx<'a> {
    type Error = anyhow::Error;
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for Address {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::address, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for AssetId {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::asset_id, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for ContractId {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::contract_id, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for ScriptCode {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::script_code, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for PredicateCode {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::script_code, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for UtxoId {
    async fn decompress_with(
        c: &CompressedUtxoId,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.lookup.utxo_id(c).await
    }
}

impl<'a, Specification> DecompressibleBy<DecompressCtx<'a>> for Coin<Specification>
where
    Specification: CoinSpecification,
    Specification::Predicate: DecompressibleBy<DecompressCtx<'a>>,
    Specification::PredicateData: DecompressibleBy<DecompressCtx<'a>>,
    Specification::PredicateGasUsed: DecompressibleBy<DecompressCtx<'a>>,
    Specification::Witness: DecompressibleBy<DecompressCtx<'a>>,
{
    async fn decompress_with(
        c: &<Coin<Specification> as Compressible>::Compressed,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Coin<Specification>> {
        let utxo_id = UtxoId::decompress_with(&c.utxo_id, ctx).await?;
        let coin_info = ctx.lookup.coin(&utxo_id).await?;
        let witness_index = c.witness_index.decompress(ctx).await?;
        let predicate_gas_used = c.predicate_gas_used.decompress(ctx).await?;
        let predicate = c.predicate.decompress(ctx).await?;
        let predicate_data = c.predicate_data.decompress(ctx).await?;
        Ok(Self {
            utxo_id,
            owner: coin_info.owner,
            amount: coin_info.amount,
            asset_id: coin_info.asset_id,
            tx_pointer: Default::default(),
            witness_index,
            predicate_gas_used,
            predicate,
            predicate_data,
        })
    }
}

impl<'a, Specification> DecompressibleBy<DecompressCtx<'a>> for Message<Specification>
where
    Specification: MessageSpecification,
    Specification::Data: DecompressibleBy<DecompressCtx<'a>> + Default,
    Specification::Predicate: DecompressibleBy<DecompressCtx<'a>>,
    Specification::PredicateData: DecompressibleBy<DecompressCtx<'a>>,
    Specification::PredicateGasUsed: DecompressibleBy<DecompressCtx<'a>>,
    Specification::Witness: DecompressibleBy<DecompressCtx<'a>>,
{
    async fn decompress_with(
        c: &<Message<Specification> as Compressible>::Compressed,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Message<Specification>> {
        let msg = ctx.lookup.message(&c.nonce).await?;
        let witness_index = c.witness_index.decompress(ctx).await?;
        let predicate_gas_used = c.predicate_gas_used.decompress(ctx).await?;
        let predicate = c.predicate.decompress(ctx).await?;
        let predicate_data = c.predicate_data.decompress(ctx).await?;
        let mut message: Message<Specification> = Message {
            sender: msg.sender,
            recipient: msg.recipient,
            amount: msg.amount,
            nonce: c.nonce,
            witness_index,
            predicate_gas_used,
            data: Default::default(),
            predicate,
            predicate_data,
        };

        if let Some(data) = message.data.as_mut_field() {
            data.clone_from(&msg.data)
        }

        Ok(message)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>> for Mint {
    async fn decompress_with(
        c: &Self::Compressed,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        Ok(Transaction::mint(
            Default::default(), // TODO: what should this we do with this?
            c.input_contract.decompress(ctx).await?,
            c.output_contract.decompress(ctx).await?,
            c.mint_amount.decompress(ctx).await?,
            c.mint_asset_id.decompress(ctx).await?,
            c.gas_price.decompress(ctx).await?,
        ))
    }
}
