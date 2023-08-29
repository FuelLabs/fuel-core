#![allow(missing_docs)]

mod counts;
mod pressure_block_importer;
mod pressure_consensus;
mod pressure_peer_to_peer;

use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Sealed,
        },
        header::BlockHeader,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
};

pub use counts::{
    Count,
    SharedCounts,
};
use fuel_core_types::services::p2p::{
    PeerId,
    SourcePeer,
};
pub use pressure_block_importer::PressureBlockImporter;
pub use pressure_consensus::PressureConsensus;
pub use pressure_peer_to_peer::PressurePeerToPeer;

pub fn empty_header(h: BlockHeight) -> SealedBlockHeader {
    let mut header = BlockHeader::default();
    header.consensus.height = h;
    let transaction_tree =
        fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new();
    header.application.generated.transactions_root = transaction_tree.root().into();

    let consensus = Consensus::default();
    Sealed {
        entity: header,
        consensus,
    }
}

pub fn peer_sourced_headers(
    headers: Option<Vec<SealedBlockHeader>>,
) -> SourcePeer<Option<Vec<SealedBlockHeader>>> {
    peer_sourced_headers_peer_id(headers, vec![].into())
}

pub fn peer_sourced_headers_peer_id(
    headers: Option<Vec<SealedBlockHeader>>,
    peer_id: PeerId,
) -> SourcePeer<Option<Vec<SealedBlockHeader>>> {
    SourcePeer {
        peer_id,
        data: headers,
    }
}
