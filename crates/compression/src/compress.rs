use crate::{
    ports::UtxoIdToPointer,
    tables::{
        PrepareCtx,
        RegistrationsPerTable,
        TemporalRegistryAll,
    },
    CompressedBlock,
    CompressedBlockPayloadV0,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_compression::CompressibleBy,
    fuel_tx::Bytes32,
};

pub trait CompressDb: TemporalRegistryAll + UtxoIdToPointer {}
impl<T> CompressDb for T where T: TemporalRegistryAll + UtxoIdToPointer {}

/// This must be called for all new blocks in sequence, otherwise the result will be garbage.
pub async fn compress<D>(mut db: D, block: &Block) -> anyhow::Result<Vec<u8>>
where
    D: CompressDb,
{
    let target = block.transactions_vec();

    let mut prepare_ctx = PrepareCtx::new(&mut db);
    let _ = target.compress_with(&mut prepare_ctx).await?;

    let mut ctx = prepare_ctx.into_compression_context();
    let transactions = target.compress_with(&mut ctx).await?;
    let registrations: RegistrationsPerTable = ctx.into_registrations();

    // Apply changes to the db
    registrations.write_to_registry(&mut db)?;

    // Construct the actual compacted block
    let compact = CompressedBlockPayloadV0 {
        registrations,
        registrations_root: Bytes32::default(), /* TODO: https://github.com/FuelLabs/fuel-core/issues/2232 */
        header: block.header().into(),
        transactions,
    };

    let compressed = postcard::to_allocvec(&CompressedBlock::V0(compact))
        .expect("Serialization cannot fail");

    Ok(compressed)
}
