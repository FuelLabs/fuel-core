use fuel_core_types::{
    blockchain::block::Block,
    fuel_compression::CompressibleBy,
    fuel_tx::{
        Bytes32,
        Transaction,
    },
};

use crate::{
    context::{
        compress::CompressCtx,
        prepare::PrepareCtx,
    },
    eviction_policy::CacheEvictor,
    ports::{
        EvictorDb,
        UtxoIdToPointer,
    },
    tables::{
        PerRegistryKeyspace,
        RegistrationsPerTable,
        TemporalRegistryAll,
    },
    CompressedBlockPayload,
    Header,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Only the next sequential block can be compressed")]
    NotLatest,
    #[error("Unknown compression error")]
    Other(#[from] anyhow::Error),
}

pub trait CompressDb: TemporalRegistryAll + UtxoIdToPointer {}
impl<T> CompressDb for T where T: TemporalRegistryAll + UtxoIdToPointer {}

pub async fn compress<D: CompressDb + EvictorDb>(
    db: D,
    block: &Block,
) -> Result<Vec<u8>, Error> {
    // if *block.header().height() != db.next_block_height()? {
    //     return Err(Error::NotLatest);
    // }

    let target = block.transactions().to_vec();

    let mut prepare_ctx = PrepareCtx {
        db,
        accessed_keys: PerRegistryKeyspace::default(),
    };
    let _ =
        <Vec<Transaction> as CompressibleBy<_>>::compress_with(&target, &mut prepare_ctx)
            .await?;

    let mut ctx = CompressCtx {
        db: prepare_ctx.db,
        cache_evictor: CacheEvictor {
            keep_keys: prepare_ctx.accessed_keys,
        },
        changes: Default::default(),
    };
    let transactions = target.compress_with(&mut ctx).await?;
    let registrations = ctx.changes;
    let registrations = RegistrationsPerTable::try_from(registrations)?;

    // Apply changes to the db
    registrations.write_to_registry(&mut ctx.db)?;

    // Construct the actual compacted block
    let compact = CompressedBlockPayload {
        registrations,
        registrations_root: Bytes32::default(), /* TODO: https://github.com/FuelLabs/fuel-core/issues/2232 */
        header: Header {
            da_height: block.header().da_height,
            prev_root: *block.header().prev_root(),
            consensus_parameters_version: block.header().consensus_parameters_version,
            state_transition_bytecode_version: block
                .header()
                .state_transition_bytecode_version,
            height: *block.header().height(),
            time: block.header().time(),
        },
        transactions,
    };

    let version = 0u8;

    let compressed =
        postcard::to_allocvec(&(version, compact)).expect("Serialization cannot fail");

    Ok(compressed)
}
