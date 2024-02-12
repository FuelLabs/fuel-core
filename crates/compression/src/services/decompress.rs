use tokio::sync::mpsc;

use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            BlockV1,
            PartialFuelBlock,
        },
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::Empty,
    },
    fuel_compression::Compactable,
    fuel_tx::Transaction,
};

use crate::{
    db::TemporalRegistry,
    CompressedBlock,
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

async fn run<R: TemporalRegistry>(
    mut db: R,
    mut request_receiver: mpsc::Receiver<TaskRequest>,
) {
    while let Some(req) = request_receiver.recv().await {
        match req {
            TaskRequest::Decompress { block, response } => {
                let reply = decompress(&mut db, block);
                response.send(reply).await.expect("Failed to respond");
            }
        }
    }
}

fn decompress<R: TemporalRegistry>(
    db: &mut R,
    block: Vec<u8>,
) -> Result<PartialFuelBlock, DecompressError> {
    if block.is_empty() || block[0] != 0 {
        return Err(DecompressError::UnknownVersion);
    }

    let compressed: CompressedBlock = postcard::from_bytes(&block[1..])?;

    // TODO: should be store height on da just to have this check?
    // if *block.header.height != db.next_block_height()? {
    //     return Err(DecompressError::NotLatest);
    // }

    let mut transactions = Vec::new();
    for tx in compressed.transactions.into_iter() {
        transactions.push(Transaction::decompact(tx, db)?);
    }

    Ok(PartialFuelBlock {
        header: PartialBlockHeader {
            application: ApplicationHeader {
                da_height: compressed.header.da_height,
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
