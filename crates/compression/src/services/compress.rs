use fuel_core_types::{
    fuel_compression::CompressibleBy,
    fuel_tx::Transaction,
};
use tokio::sync::mpsc;

use fuel_core_types::blockchain::block::Block;

use crate::{
    context::{
        compress::CompressCtx,
        prepare::PrepareCtx,
    },
    db::RocksDb,
    eviction_policy::CacheEvictor,
    ports::TxIdToPointer,
    tables::{
        PerRegistryKeyspace,
        RegistrationsPerTable,
    },
    CompressedBlockPayload,
    Header,
};

/// Task handle
pub struct Task {
    request_receiver: mpsc::Receiver<TaskRequest>,
}

pub enum TaskRequest {
    Compress {
        block: Block,
        response: mpsc::Sender<Result<Vec<u8>, CompressError>>,
    },
}

#[derive(Debug)]
pub enum CompressError {
    /// Only the next sequential block can be compressed
    NotLatest,
    Other(anyhow::Error),
}
impl From<anyhow::Error> for CompressError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}

pub async fn run(
    mut db: RocksDb,
    tx_lookup: Box<dyn TxIdToPointer>,
    mut request_receiver: mpsc::Receiver<TaskRequest>,
) {
    while let Some(req) = request_receiver.recv().await {
        match req {
            TaskRequest::Compress { block, response } => {
                let reply = compress(&mut db, &*tx_lookup, block).await;
                response.send(reply).await.expect("Failed to respond");
            }
        }
    }
}

pub async fn compress(
    db: &mut RocksDb,
    tx_lookup: &dyn TxIdToPointer,
    block: Block,
) -> Result<Vec<u8>, CompressError> {
    if *block.header().height() != db.next_block_height()? {
        return Err(CompressError::NotLatest);
    }

    let target = block.transactions().to_vec();

    let mut prepare_ctx = PrepareCtx {
        db,
        accessed_keys: PerRegistryKeyspace::default(),
    };
    let _ =
        <Vec<Transaction> as CompressibleBy<_, _>>::compress(&target, &mut prepare_ctx)
            .await?;

    let mut ctx = CompressCtx {
        db: prepare_ctx.db,
        tx_lookup,
        cache_evictor: CacheEvictor {
            keep_keys: prepare_ctx.accessed_keys,
        },
        changes: Default::default(),
    };
    let transactions = target.compress(&mut ctx).await?;
    let registrations = ctx.changes;
    let registrations = RegistrationsPerTable::try_from(registrations)?;

    // Apply changes to the db
    // TODO: these two operations should be atomic together
    registrations.write_to_db(db)?;
    db.increment_block_height()?;

    // Construct the actual compacted block
    let compact = CompressedBlockPayload {
        registrations,
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
