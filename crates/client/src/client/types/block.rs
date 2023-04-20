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
    fuel_crypto,
    fuel_types::BlockHeight,
};

#[derive(Debug)]
pub struct Block {
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<TransactionId>,
}

impl Block {
    /// Returns the block producer public key, if any.
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

//

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
        match value {
            schema::block::Consensus::Genesis(genesis) => {
                Consensus::Genesis(genesis.into())
            }
            schema::block::Consensus::PoAConsensus(poa) => {
                Consensus::PoAConsensus(poa.into())
            }
            schema::block::Consensus::Unknown => Consensus::Unknown,
        }
    }
}

impl From<schema::block::Genesis> for Genesis {
    fn from(value: schema::block::Genesis) -> Self {
        Self {
            chain_config_hash: value.chain_config_hash.0 .0.into(),
            coins_root: value.coins_root.0 .0.into(),
            contracts_root: value.contracts_root.0 .0.into(),
            messages_root: value.messages_root.0 .0.into(),
        }
    }
}

impl From<schema::block::PoAConsensus> for PoAConsensus {
    fn from(value: schema::block::PoAConsensus) -> Self {
        Self {
            signature: value.signature.0 .0.into(),
        }
    }
}

impl From<schema::block::Block> for Block {
    fn from(value: schema::block::Block) -> Self {
        let transactions = value
            .transactions
            .iter()
            .map(|tx| tx.id.0 .0)
            .map(Into::into)
            .collect::<Vec<TransactionId>>();
        Self {
            id: value.id.into(),
            header: value.header.into(),
            consensus: value.consensus.into(),
            transactions,
        }
    }
}
