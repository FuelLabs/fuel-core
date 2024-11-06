use crate::{
    config::Config,
    ports::{
        HistoryLookup,
        TemporalRegistry,
    },
    registry::TemporalRegistryAll,
    VersionedCompressedBlock,
};
use fuel_core_types::{
    blockchain::block::PartialFuelBlock,
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
        AssetId,
        CompressedUtxoId,
        Mint,
        ScriptCode,
        Transaction,
        UtxoId,
    },
    fuel_types::{
        Address,
        ContractId,
    },
    tai64::Tai64,
};

pub trait DecompressDb: TemporalRegistryAll + HistoryLookup {}
impl<T> DecompressDb for T where T: TemporalRegistryAll + HistoryLookup {}

/// This must be called for all decompressed blocks in sequence, otherwise the result will be garbage.
pub async fn decompress<D>(
    config: Config,
    mut db: D,
    block: VersionedCompressedBlock,
) -> anyhow::Result<PartialFuelBlock>
where
    D: DecompressDb,
{
    let VersionedCompressedBlock::V0(compressed) = block;

    // TODO: merkle root verification: https://github.com/FuelLabs/fuel-core/issues/2232

    compressed
        .registrations
        .write_to_registry(&mut db, compressed.header.consensus.time)?;

    let ctx = DecompressCtx {
        config,
        timestamp: compressed.header.consensus.time,
        db,
    };

    let transactions = <Vec<Transaction> as DecompressibleBy<_>>::decompress_with(
        compressed.transactions,
        &ctx,
    )
    .await?;

    Ok(PartialFuelBlock {
        header: compressed.header,
        transactions,
    })
}

pub struct DecompressCtx<D> {
    pub config: Config,
    /// Timestamp of the block being decompressed
    pub timestamp: Tai64,
    pub db: D,
}

impl<D> ContextError for DecompressCtx<D> {
    type Error = anyhow::Error;
}

impl<D> DecompressibleBy<DecompressCtx<D>> for UtxoId
where
    D: HistoryLookup,
{
    async fn decompress_with(
        c: CompressedUtxoId,
        ctx: &DecompressCtx<D>,
    ) -> anyhow::Result<Self> {
        ctx.db.utxo_id(c)
    }
}

macro_rules! decompress_impl {
    ($($type:ty),*) => { paste::paste! {
        $(
            impl<D> DecompressibleBy<DecompressCtx<D>> for $type
            where
                D: TemporalRegistry<$type>
            {
                async fn decompress_with(
                    key: RegistryKey,
                    ctx: &DecompressCtx<D>,
                ) -> anyhow::Result<Self> {
                    if key == RegistryKey::DEFAULT_VALUE {
                        return Ok(<$type>::default());
                    }
                    let key_timestamp = ctx.db.read_timestamp(&key)?;
                    if !ctx.config.is_timestamp_accessible(ctx.timestamp, key_timestamp)? {
                        anyhow::bail!("Timestamp not accessible");
                    }
                    ctx.db.read_registry(&key)
                }
            }
        )*
    }};
}

decompress_impl!(AssetId, ContractId, Address, PredicateCode, ScriptCode);

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
    ) -> anyhow::Result<Coin<Specification>> {
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
    ) -> anyhow::Result<Message<Specification>> {
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

impl<D> DecompressibleBy<DecompressCtx<D>> for Mint
where
    D: DecompressDb,
{
    async fn decompress_with(
        c: Self::Compressed,
        ctx: &DecompressCtx<D>,
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

#[cfg(test)]
mod tests {
    use crate::ports::{
        EvictorDb,
        TemporalRegistry,
    };

    use super::*;
    use fuel_core_types::{
        fuel_compression::RegistryKey,
        fuel_tx::{
            input::PredicateCode,
            Address,
            AssetId,
            ContractId,
            ScriptCode,
        },
    };
    use serde::{
        Deserialize,
        Serialize,
    };

    pub struct MockDb;
    impl HistoryLookup for MockDb {
        fn utxo_id(&self, _: CompressedUtxoId) -> anyhow::Result<UtxoId> {
            unimplemented!()
        }

        fn coin(&self, _: UtxoId) -> anyhow::Result<crate::ports::CoinInfo> {
            unimplemented!()
        }

        fn message(
            &self,
            _: fuel_core_types::fuel_types::Nonce,
        ) -> anyhow::Result<crate::ports::MessageInfo> {
            unimplemented!()
        }
    }
    macro_rules! mock_temporal {
        ($type:ty) => {
            impl TemporalRegistry<$type> for MockDb {
                fn read_registry(&self, _key: &RegistryKey) -> anyhow::Result<$type> {
                    unimplemented!()
                }

                fn read_timestamp(&self, _key: &RegistryKey) -> anyhow::Result<Tai64> {
                    unimplemented!()
                }

                fn write_registry(
                    &mut self,
                    _key: &RegistryKey,
                    _value: &$type,
                    _timestamp: Tai64,
                ) -> anyhow::Result<()> {
                    unimplemented!()
                }

                fn registry_index_lookup(
                    &self,
                    _value: &$type,
                ) -> anyhow::Result<Option<RegistryKey>> {
                    unimplemented!()
                }
            }

            impl EvictorDb<$type> for MockDb {
                fn set_latest_assigned_key(
                    &mut self,
                    _key: RegistryKey,
                ) -> anyhow::Result<()> {
                    unimplemented!()
                }

                fn get_latest_assigned_key(&self) -> anyhow::Result<Option<RegistryKey>> {
                    unimplemented!()
                }
            }
        };
    }
    mock_temporal!(Address);
    mock_temporal!(AssetId);
    mock_temporal!(ContractId);
    mock_temporal!(ScriptCode);
    mock_temporal!(PredicateCode);

    #[tokio::test]
    async fn decompress_block_with_unknown_version() {
        #[derive(Clone, Serialize, Deserialize)]
        enum CompressedBlockWithNewVersions {
            V0(crate::CompressedBlockPayloadV0),
            NewVersion(u32),
            #[serde(untagged)]
            Unknown,
        }

        // Given
        let bad_block =
            postcard::to_stdvec(&CompressedBlockWithNewVersions::NewVersion(1234))
                .unwrap();

        // When
        let result: Result<VersionedCompressedBlock, _> =
            postcard::from_bytes(&bad_block);

        // Then
        let _ =
            result.expect_err("should fail to deserialize because of unknown version");
    }
}
