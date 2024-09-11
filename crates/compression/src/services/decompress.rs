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
    fuel_compression::DecompressibleBy,
    fuel_tx::Transaction,
};

use crate::{
    context::decompress::DecompressCtx,
    ports::{
        HistoryLookup,
        TemporalRegistry,
    },
    CompressedBlockPayload,
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

pub trait DecompressDb: TemporalRegistry + HistoryLookup {}
impl<T> DecompressDb for T where T: TemporalRegistry + HistoryLookup {}

pub async fn decompress<D: DecompressDb + TemporalRegistry>(
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

    compressed.registrations.write_to_registry(&mut db)?;

    let ctx = DecompressCtx { db };

    let transactions = <Vec<Transaction> as DecompressibleBy<_>>::decompress_with(
        &compressed.transactions,
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
