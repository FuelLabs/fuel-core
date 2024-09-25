use fuel_core_types::{
    fuel_compression::{
        Compressible,
        ContextError,
        Decompress,
        DecompressibleBy,
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
        },
        CompressedUtxoId,
        Mint,
        Transaction,
        UtxoId,
    },
};

use crate::decompress::{
    DecompressDb,
    DecompressError,
};

pub struct DecompressCtx<D> {
    pub db: D,
}

impl<D: DecompressDb> ContextError for DecompressCtx<D> {
    type Error = DecompressError;
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for UtxoId {
    async fn decompress_with(
        c: CompressedUtxoId,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
        Ok(ctx.db.utxo_id(c)?)
    }
}

impl<D, Specification> DecompressibleBy<DecompressCtx<D>> for Coin<Specification>
where
    D: DecompressDb,
    Specification: CoinSpecification,
    Specification::Predicate: DecompressibleBy<DecompressCtx<D>>,
    Specification::PredicateData: DecompressibleBy<DecompressCtx<D>>,
    Specification::PredicateGasUsed: DecompressibleBy<DecompressCtx<D>>,
    Specification::Witness: DecompressibleBy<DecompressCtx<D>>,
{
    async fn decompress_with(
        c: <Coin<Specification> as Compressible>::Compressed,
        ctx: &DecompressCtx<D>,
    ) -> Result<Coin<Specification>, DecompressError> {
        let utxo_id = UtxoId::decompress_with(c.utxo_id, ctx).await?;
        let coin_info = ctx.db.coin(utxo_id)?;
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

impl<D, Specification> DecompressibleBy<DecompressCtx<D>> for Message<Specification>
where
    D: DecompressDb,
    Specification: MessageSpecification,
    Specification::Data: DecompressibleBy<DecompressCtx<D>> + Default,
    Specification::Predicate: DecompressibleBy<DecompressCtx<D>>,
    Specification::PredicateData: DecompressibleBy<DecompressCtx<D>>,
    Specification::PredicateGasUsed: DecompressibleBy<DecompressCtx<D>>,
    Specification::Witness: DecompressibleBy<DecompressCtx<D>>,
{
    async fn decompress_with(
        c: <Message<Specification> as Compressible>::Compressed,
        ctx: &DecompressCtx<D>,
    ) -> Result<Message<Specification>, DecompressError> {
        let msg = ctx.db.message(c.nonce)?;
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

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for Mint {
    async fn decompress_with(
        c: Self::Compressed,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
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
