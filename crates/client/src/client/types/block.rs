// use crate::client::types::primitives::BlockId;

use crate::client::types::primitives::{
    BlockId,
    Bytes32,
    MerkleRoot,
    Signature,
    Tai64Timestamp,
    TransactionId,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::BlockHeight,
};

pub struct Block {
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<TransactionId>,
}

pub struct Header {
    pub id: BlockId,
    pub da_height: DaBlockHeight,
    pub transactions_count: u64,
    pub message_receipt_count: u64,
    pub transactions_root: MerkleRoot,
    pub message_receipt_root: MerkleRoot,
    pub height: BlockHeight,
    pub prev_root: MerkleRoot,
    pub time: Tai64Timestamp,
    pub application_hash: Bytes32,
}

pub enum Consensus {
    Genesis(Genesis),
    PoAConsensus(PoAConsensus),
    Unknown,
}

pub struct Genesis {
    pub chain_config_hash: Bytes32,
    pub coins_root: MerkleRoot,
    pub contracts_root: MerkleRoot,
    pub messages_root: MerkleRoot,
}

pub struct PoAConsensus {
    pub signature: Signature,
}
