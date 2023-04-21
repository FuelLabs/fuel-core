use crate::client::types::primitives::{
    base::Bytes32,
    BlockId,
    MerkleRoot,
    Signature,
    Tai64Timestamp,
    TransactionId,
};
use fuel_core_types::fuel_crypto;

#[derive(Debug)]
pub struct Block {
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<TransactionId>,
}

impl Block {
    pub fn block_producer(&self) -> Option<fuel_crypto::PublicKey> {
        let message = self.header.id.clone().into_message();
        match &self.consensus {
            Consensus::Genesis(_) => Some(Default::default()),
            Consensus::PoAConsensus(poa) => {
                let signature = poa.signature.clone().into_signature();
                let producer_pub_key = signature.recover(&message);
                producer_pub_key.ok()
            }
            Consensus::Unknown => None,
        }
    }
}

#[derive(Debug)]
pub struct Header {
    pub id: BlockId,
    pub da_height: u64,
    pub transactions_count: u64,
    pub message_receipt_count: u64,
    pub transactions_root: MerkleRoot,
    pub message_receipt_root: MerkleRoot,
    pub height: u32,
    pub prev_root: MerkleRoot,
    pub time: Tai64Timestamp,
    pub application_hash: Bytes32,
}

#[derive(Debug)]
pub enum Consensus {
    Genesis(Genesis),
    PoAConsensus(PoAConsensus),
    Unknown,
}

#[derive(Debug)]
pub struct Genesis {
    pub chain_config_hash: Bytes32,
    pub coins_root: MerkleRoot,
    pub contracts_root: MerkleRoot,
    pub messages_root: MerkleRoot,
}

#[derive(Debug)]
pub struct PoAConsensus {
    pub signature: Signature,
}
