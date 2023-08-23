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
    services::p2p::SourcePeer,
};

pub use counts::{
    Count,
    SharedCounts,
};
pub use pressure_block_importer::PressureBlockImporter;
pub use pressure_consensus::PressureConsensus;
pub use pressure_peer_to_peer::PressurePeerToPeer;

pub fn empty_header(h: BlockHeight) -> SourcePeer<SealedBlockHeader> {
    let mut header = BlockHeader::default();
    header.consensus.height = h;
    let transaction_tree =
        fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new();
    header.application.generated.transactions_root = transaction_tree.root().into();

    let consensus = Consensus::default();
    let sealed = Sealed {
        entity: header,
        consensus,
    };
    SourcePeer {
        peer_id: vec![].into(),
        data: sealed,
    }
}
