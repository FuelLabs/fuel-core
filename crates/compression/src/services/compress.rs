use tokio::sync::mpsc;

use fuel_core_types::blockchain::block::Block;

use crate::{
    db::TemporalRegistry,
    CompressedBlock,
    Header,
};

use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_compression::{
        Compactable,
        CompactionContext,
        InMemoryRegistry,
        RegistryDb,
    },
    fuel_tx::Transaction,
    tai64::Tai64,
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

async fn run<R: TemporalRegistry>(
    mut db: R,
    mut request_receiver: mpsc::Receiver<TaskRequest>,
) {
    while let Some(req) = request_receiver.recv().await {
        match req {
            TaskRequest::Compress { block, response } => {
                let reply = compress(&mut db, block);
                response.send(reply).await.expect("Failed to respond");
            }
        }
    }
}

fn compress<R: TemporalRegistry>(
    db: &mut R,
    block: Block,
) -> Result<Vec<u8>, CompressError> {
    if *block.header().height() != db.next_block_height()? {
        return Err(CompressError::NotLatest);
    }

    let (transactions, registrations) =
        CompactionContext::run(db, block.transactions().to_vec())?;

    let compact = CompressedBlock {
        registrations,
        header: Header {
            da_height: block.header().da_height,
            prev_root: *block.header().prev_root(),
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
