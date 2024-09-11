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
    services::decompress::{
        DecompressDb,
        DecompressError,
    },
    tables::RegistryKeyspace,
};

pub struct DecompressCtx<D> {
    pub db: D,
}

impl<D: DecompressDb> ContextError for DecompressCtx<D> {
    type Error = DecompressError;
}

fn registry_desubstitute<
    D: DecompressDb,
    T: serde::de::DeserializeOwned + Default + PartialEq,
>(
    keyspace: RegistryKeyspace,
    key: RegistryKey,
    ctx: &DecompressCtx<D>,
) -> Result<T, DecompressError> {
    if key == RegistryKey::DEFAULT_VALUE {
        return Ok(T::default());
    }
    Ok(postcard::from_bytes(&ctx.db.read_registry(keyspace, key)?)?)
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for Address {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
        registry_desubstitute(RegistryKeyspace::address, *c, ctx)
    }
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for AssetId {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
        registry_desubstitute(RegistryKeyspace::asset_id, *c, ctx)
    }
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for ContractId {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
        registry_desubstitute(RegistryKeyspace::contract_id, *c, ctx)
    }
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for ScriptCode {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
        registry_desubstitute(RegistryKeyspace::script_code, *c, ctx)
    }
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for PredicateCode {
    async fn decompress_with(
        c: &RegistryKey,
        ctx: &DecompressCtx<D>,
    ) -> Result<Self, DecompressError> {
        registry_desubstitute(RegistryKeyspace::predicate_code, *c, ctx)
    }
}

impl<D: DecompressDb> DecompressibleBy<DecompressCtx<D>> for UtxoId {
    async fn decompress_with(
        c: &CompressedUtxoId,
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
        c: &<Coin<Specification> as Compressible>::Compressed,
        ctx: &DecompressCtx<D>,
    ) -> Result<Coin<Specification>, DecompressError> {
        let utxo_id = UtxoId::decompress_with(&c.utxo_id, ctx).await?;
        let coin_info = ctx.db.coin(&utxo_id)?;
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
        c: &<Message<Specification> as Compressible>::Compressed,
        ctx: &DecompressCtx<D>,
    ) -> Result<Message<Specification>, DecompressError> {
        let msg = ctx.db.message(&c.nonce)?;
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
        c: &Self::Compressed,
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
