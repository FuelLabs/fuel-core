use crate::{
    ports::HistoryLookup,
    tables::TemporalRegistryAll,
    CompressedBlockPayload,
};
use fuel_core_types::{
    blockchain::{
        block::PartialFuelBlock,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::Empty,
    },
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

#[derive(Debug, thiserror::Error)]
pub enum DecompressError {
    #[error("Only the next sequential block can be decompressed")]
    NotLatest,
    #[error("Unknown compression version")]
    UnknownVersion,
    #[error("Deserialization error: {0}")]
    Postcard(#[from] postcard::Error),
    /// Other errors
    #[error("Unknown error: {0}")]
    Other(#[from] anyhow::Error),
}

pub trait DecompressDb: TemporalRegistryAll + HistoryLookup {}
impl<T> DecompressDb for T where T: TemporalRegistryAll + HistoryLookup {}

pub async fn decompress<D: DecompressDb + TemporalRegistryAll>(
    mut db: D,
    block: Vec<u8>,
) -> Result<PartialFuelBlock, DecompressError> {
    if block.is_empty() || block[0] != 0 {
        return Err(DecompressError::UnknownVersion);
    }

    let compressed: CompressedBlockPayload = postcard::from_bytes(&block[1..])?;

    // TODO: should be store height on da just to have this check?
    // if *block.header.height != db.next_block_height()? {
    //     return Err(DecompressError::NotLatest);
    // }

    // TODO: merkle root verification: https://github.com/FuelLabs/fuel-core/issues/2232

    compressed.registrations.write_to_registry(&mut db)?;

    let ctx = DecompressCtx { db };

    let transactions = <Vec<Transaction> as DecompressibleBy<_>>::decompress_with(
        compressed.transactions,
        &ctx,
    )
    .await?;

    Ok(PartialFuelBlock {
        header: PartialBlockHeader {
            application: ApplicationHeader {
                da_height: compressed.header.da_height,
                consensus_parameters_version: compressed
                    .header
                    .consensus_parameters_version,
                state_transition_bytecode_version: compressed
                    .header
                    .state_transition_bytecode_version,
                generated: Empty,
            },
            consensus: ConsensusHeader {
                prev_root: compressed.header.prev_root,
                height: compressed.header.height,
                time: compressed.header.time,
                generated: Empty,
            },
        },
        transactions,
    })
}

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
