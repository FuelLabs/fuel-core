use fuel_core_types::fuel_compression::{
    tables,
    DecompactionContext,
    Key,
    Table,
};
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
    db::RocksDb,
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

async fn run(mut db: RocksDb, mut request_receiver: mpsc::Receiver<TaskRequest>) {
    while let Some(req) = request_receiver.recv().await {
        match req {
            TaskRequest::Decompress { block, response } => {
                let reply = decompress(&mut db, block);
                response.send(reply).await.expect("Failed to respond");
            }
        }
    }
}

fn decompress(
    db: &mut RocksDb,
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

    let ctx = Ctx { db };

    let mut transactions = Vec::new();
    for tx in compressed.transactions.into_iter() {
        transactions.push(Transaction::decompact(tx, &ctx)?);
    }

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

pub struct Ctx<'a> {
    db: &'a RocksDb,
}

impl DecompactionContext for Ctx<'_> {
    fn read_AssetId(
        &self,
        key: Key<tables::AssetId>,
    ) -> anyhow::Result<<tables::AssetId as Table>::Type> {
        todo!()
    }

    fn read_Address(
        &self,
        key: Key<tables::Address>,
    ) -> anyhow::Result<<tables::Address as Table>::Type> {
        todo!()
    }

    fn read_ContractId(
        &self,
        key: Key<tables::ContractId>,
    ) -> anyhow::Result<<tables::ContractId as Table>::Type> {
        todo!()
    }

    fn read_ScriptCode(
        &self,
        key: Key<tables::ScriptCode>,
    ) -> anyhow::Result<<tables::ScriptCode as Table>::Type> {
        todo!()
    }

    fn read_Witness(
        &self,
        key: Key<tables::Witness>,
    ) -> anyhow::Result<<tables::Witness as Table>::Type> {
        todo!()
    }
}
