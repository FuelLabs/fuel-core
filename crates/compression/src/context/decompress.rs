use fuel_core_types::{
    fuel_asm::Word,
    fuel_compression::{
        DecompressibleBy,
        RegistryKey,
    },
    fuel_tx::{
        input::{
            self,
            coin::{
                self,
                Coin,
                CompressedCoin,
            },
            message::{
                self,
                CompressedMessage,
                Message,
            },
            Empty,
            PredicateCode,
        },
        output,
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

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for Address {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::address, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for AssetId {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::asset_id, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for ContractId {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::contract_id, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for ScriptCode {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::script_code, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for PredicateCode {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.db.read_registry(RegistryKeyspace::script_code, *c)
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for UtxoId {
    async fn decompress_with(
        c: &CompressedUtxoId,
        ctx: &DecompressCtx<'a>,
    ) -> anyhow::Result<Self> {
        ctx.lookup.utxo_id(c).await
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for Mint {
    async fn decompress_with(
        c: &Self::Compressed,
        ctx: &DecompressCtx<'a>,
    ) -> Result<Self, anyhow::Error> {
        Ok(Transaction::mint(
            Default::default(), // TODO: what should this we do with this?
            <input::contract::Contract as DecompressibleBy<_, anyhow::Error>>::decompress_with(
                &c.input_contract,
                ctx,
            )
            .await?,
            <output::contract::Contract as DecompressibleBy<_, anyhow::Error>>::decompress_with(
                &c.output_contract,
                ctx,
            )
            .await?,
            <Word as DecompressibleBy<_, anyhow::Error>>::decompress_with(&c.mint_amount, ctx).await?,
            <AssetId as DecompressibleBy<_, anyhow::Error>>::decompress_with(&c.mint_asset_id, ctx).await?,
            <Word as DecompressibleBy<_, anyhow::Error>>::decompress_with(&c.gas_price, ctx).await?,
        ))
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for Coin<coin::Full> {
    async fn decompress_with(
        c: &CompressedCoin<coin::Full>,
        ctx: &DecompressCtx<'a>,
    ) -> Result<Coin<coin::Full>, anyhow::Error> {
        let utxo_id = UtxoId::decompress_with(&c.utxo_id, ctx).await?;
        let coin_info = ctx.lookup.coin(&utxo_id).await?;
        Ok(Coin {
            utxo_id,
            owner: coin_info.owner,
            amount: coin_info.amount,
            asset_id: coin_info.asset_id,
            tx_pointer: Default::default(),
            witness_index: c.witness_index,
            predicate_gas_used: c.predicate_gas_used,
            predicate:
                <coin::Full as coin::CoinSpecification>::Predicate::decompress_with(
                    &c.predicate,
                    ctx,
                )
                .await?,
            predicate_data: c.predicate_data.clone(),
        })
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for Coin<coin::Signed> {
    async fn decompress_with(
        c: &CompressedCoin<coin::Signed>,
        ctx: &DecompressCtx<'a>,
    ) -> Result<Coin<coin::Signed>, anyhow::Error> {
        let utxo_id = UtxoId::decompress_with(&c.utxo_id, ctx).await?;
        let coin_info = ctx.lookup.coin(&utxo_id).await?;
        Ok(Coin {
            utxo_id,
            owner: coin_info.owner,
            amount: coin_info.amount,
            asset_id: coin_info.asset_id,
            tx_pointer: Default::default(),
            witness_index: c.witness_index,
            predicate_gas_used: Empty::default(),
            predicate: Empty::default(),
            predicate_data: Empty::default(),
        })
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error> for Coin<coin::Predicate> {
    async fn decompress_with(
        c: &CompressedCoin<coin::Predicate>,
        ctx: &DecompressCtx<'a>,
    ) -> Result<Coin<coin::Predicate>, anyhow::Error> {
        let utxo_id = UtxoId::decompress_with(&c.utxo_id, ctx).await?;
        let coin_info = ctx.lookup.coin(&utxo_id).await?;
        Ok(Coin {
            utxo_id,
            owner: coin_info.owner,
            amount: coin_info.amount,
            asset_id: coin_info.asset_id,
            tx_pointer: Default::default(),
            witness_index: Empty::default(),
            predicate_gas_used: c.predicate_gas_used,
            predicate:
                <coin::Full as coin::CoinSpecification>::Predicate::decompress_with(
                    &c.predicate,
                    ctx,
                )
                .await?,
            predicate_data: c.predicate_data.clone(),
        })
    }
}

impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error>
    for Message<message::specifications::Full>
{
    async fn decompress_with(
        c: &CompressedMessage<message::specifications::Full>,
        ctx: &DecompressCtx<'a>,
    ) -> Result<Message<message::specifications::Full>, anyhow::Error> {
        let msg = ctx.lookup.message(&c.nonce).await?;
        Ok(Message {
            sender: msg.sender,
            recipient: msg.recipient,
            amount: msg.amount,
            nonce: c.nonce,
            witness_index: c.witness_index,
            predicate_gas_used: c.predicate_gas_used,
            data: msg.data.clone(),
            predicate:
                <message::specifications::Full as message::MessageSpecification>::Predicate::decompress_with(
                    &c.predicate,
                    ctx,
                )
                .await?,
            predicate_data: c.predicate_data.clone(),
        })
    }
}
impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error>
    for Message<message::specifications::MessageData<message::specifications::Signed>>
{
    async fn decompress_with(
        c: &CompressedMessage<
            message::specifications::MessageData<message::specifications::Signed>,
        >,
        ctx: &DecompressCtx<'a>,
    ) -> Result<
        Message<message::specifications::MessageData<message::specifications::Signed>>,
        anyhow::Error,
    > {
        let msg = ctx.lookup.message(&c.nonce).await?;
        Ok(Message {
            sender: msg.sender,
            recipient: msg.recipient,
            amount: msg.amount,
            nonce: c.nonce,
            witness_index: c.witness_index,
            predicate_gas_used: Empty::default(),
            data: msg.data.clone(),
            predicate: <<message::specifications::MessageData<
                message::specifications::Signed,
            > as message::MessageSpecification>::Predicate as DecompressibleBy<
                _,
                anyhow::Error,
            >>::decompress_with(&c.predicate, ctx)
            .await?,
            predicate_data: Empty::default(),
        })
    }
}
impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error>
    for Message<message::specifications::MessageData<message::specifications::Predicate>>
{
    async fn decompress_with(
        c: &CompressedMessage<
            message::specifications::MessageData<message::specifications::Predicate>,
        >,
        ctx: &DecompressCtx<'a>,
    ) -> Result<
        Message<message::specifications::MessageData<message::specifications::Predicate>>,
        anyhow::Error,
    > {
        let msg = ctx.lookup.message(&c.nonce).await?;
        Ok(Message {
            sender: msg.sender,
            recipient: msg.recipient,
            amount: msg.amount,
            nonce: c.nonce,
            witness_index: Empty::default(),
            predicate_gas_used: c.predicate_gas_used,
            data: msg.data.clone(),
            predicate: <message::specifications::MessageData<
                message::specifications::Predicate,
            > as message::MessageSpecification>::Predicate::decompress_with(
                &c.predicate,
                ctx,
            )
            .await?,
            predicate_data: c.predicate_data.clone(),
        })
    }
}
impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error>
    for Message<message::specifications::MessageCoin<message::specifications::Signed>>
{
    async fn decompress_with(
        c: &CompressedMessage<
            message::specifications::MessageCoin<message::specifications::Signed>,
        >,
        ctx: &DecompressCtx<'a>,
    ) -> Result<
        Message<message::specifications::MessageCoin<message::specifications::Signed>>,
        anyhow::Error,
    > {
        let msg = ctx.lookup.message(&c.nonce).await?;
        Ok(Message {
            sender: msg.sender,
            recipient: msg.recipient,
            amount: msg.amount,
            nonce: c.nonce,
            witness_index: c.witness_index,
            predicate_gas_used: Empty::default(),
            data: Empty::default(),
            predicate: <<message::specifications::MessageCoin<
                message::specifications::Signed,
            > as message::MessageSpecification>::Predicate as DecompressibleBy<
                _,
                anyhow::Error,
            >>::decompress_with(&c.predicate, ctx)
            .await?,
            predicate_data: Empty::default(),
        })
    }
}
impl<'a> DecompressibleBy<DecompressCtx<'a>, anyhow::Error>
    for Message<message::specifications::MessageCoin<message::specifications::Predicate>>
{
    async fn decompress_with(
        c: &CompressedMessage<
            message::specifications::MessageCoin<message::specifications::Predicate>,
        >,
        ctx: &DecompressCtx<'a>,
    ) -> Result<
        Message<message::specifications::MessageCoin<message::specifications::Predicate>>,
        anyhow::Error,
    > {
        let msg = ctx.lookup.message(&c.nonce).await?;
        Ok(Message {
            sender: msg.sender,
            recipient: msg.recipient,
            amount: msg.amount,
            nonce: c.nonce,
            witness_index: Empty::default(),
            predicate_gas_used: c.predicate_gas_used,
            data: Empty::default(),
            predicate: <message::specifications::MessageCoin<
                message::specifications::Predicate,
            > as message::MessageSpecification>::Predicate::decompress_with(
                &c.predicate,
                ctx,
            )
            .await?,
            predicate_data: c.predicate_data.clone(),
        })
    }
}
