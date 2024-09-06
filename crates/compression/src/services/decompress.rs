use tokio::sync::mpsc;

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
    db::RocksDb,
    ports::HistoryLookup,
    CompressedBlockPayload,
};

/// Task handle
pub struct Task {
    request_receiver: mpsc::Receiver<TaskRequest>,
}

pub enum TaskRequest {
    Decompress {
        block: Vec<u8>,
        response: mpsc::Sender<Result<PartialFuelBlock, DecompressError>>,
    },
}

#[derive(Debug)]
pub enum DecompressError {
    /// Only the next sequential block can be decompressed
    NotLatest,
    /// Unknown compression version
    UnknownVersion,
    /// Deserialization error
    Postcard(postcard::Error),
    /// Other errors
    Other(anyhow::Error),
}
impl From<postcard::Error> for DecompressError {
    fn from(err: postcard::Error) -> Self {
        Self::Postcard(err)
    }
}
impl From<anyhow::Error> for DecompressError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}

pub async fn run(
    mut db: RocksDb,
    lookup: Box<dyn HistoryLookup>,

    mut request_receiver: mpsc::Receiver<TaskRequest>,
) {
    while let Some(req) = request_receiver.recv().await {
        match req {
            TaskRequest::Decompress { block, response } => {
                let reply = decompress(&mut db, &*lookup, block).await;
                response.send(reply).await.expect("Failed to respond");
            }
        }
    }
}

pub async fn decompress(
    db: &mut RocksDb,
    lookup: &dyn HistoryLookup,
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

    compressed.registrations.write_to_db(db)?;

    let ctx = DecompressCtx { db, lookup };

    let transactions = <Vec<Transaction> as DecompressibleBy<_, _>>::decompress_with(
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
