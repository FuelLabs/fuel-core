use crate::client::{
    schema,
    types::primitives::{
        BlockId,
        Bytes32,
        MerkleRoot,
        Signature,
        Tai64Timestamp,
        TransactionId,
    },
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

impl From<schema::block::Header> for Header {
    fn from(value: schema::block::Header) -> Self {
        Self {
            id: value.id.into(),
            da_height: value.da_height.0.into(),
            transactions_count: value.transactions_count.0,
            message_receipt_count: value.message_receipt_count.0,
            transactions_root: value.transactions_root.0 .0.into(),
            message_receipt_root: value.message_receipt_root.0 .0.into(),
            height: value.height.0.into(),
            prev_root: value.prev_root.0 .0.into(),
            time: value.time.0.into(),
            application_hash: value.application_hash.0 .0.into(),
        }
    }
}

impl From<schema::block::Consensus> for Consensus {
    fn from(value: schema::block::Consensus) -> Self {
        Self {}
    }
}

impl From<schema::block::Genesis> for Genesis {
    fn from(value: schema::block::Genesis) -> Self {
        Self {
            chain_config_hash: value.chain_config_hash.into(),
            coins_root: value.coins_root.into(),
            contracts_root: value.contracts_root.into(),
            messages_root: value.messages_root.into(),
        }
    }
}

// impl From<schema::block::Block> for Block {
//     fn from(value: schema::block::Block) -> Self {
//         Self {
//             id: value.id.into(),
//             header: value.header.into(),
//             consensus: value.consensus,
//             transactions: value.transactions,
//         }
//     }
// }
