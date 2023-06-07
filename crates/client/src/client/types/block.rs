use crate::client::{
    schema,
    types::primitives::{
        BlockId,
        Hash,
        MerkleRoot,
        PublicKey,
        Signature,
        TransactionId,
    },
    PaginatedResult,
};
use tai64::Tai64;

#[derive(Debug)]
pub struct Block {
    pub id: BlockId,
    pub header: Header,
    pub consensus: Consensus,
    pub transactions: Vec<TransactionId>,
    pub block_producer: Option<PublicKey>,
}

impl Block {
    pub fn block_producer(&self) -> Option<&PublicKey> {
        self.block_producer.as_ref()
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
    pub time: Tai64,
    pub application_hash: Hash,
}

#[derive(Debug)]
pub enum Consensus {
    Genesis(Genesis),
    PoAConsensus(PoAConsensus),
    Unknown,
}

#[derive(Debug)]
pub struct Genesis {
    pub chain_config_hash: Hash,
    pub coins_root: MerkleRoot,
    pub contracts_root: MerkleRoot,
    pub messages_root: MerkleRoot,
}

#[derive(Debug)]
pub struct PoAConsensus {
    pub signature: Signature,
}

// GraphQL Translation

impl From<schema::block::Header> for Header {
    fn from(value: schema::block::Header) -> Self {
        Self {
            id: value.id.into(),
            da_height: value.da_height.into(),
            transactions_count: value.transactions_count.into(),
            message_receipt_count: value.message_receipt_count.into(),
            transactions_root: value.transactions_root.into(),
            message_receipt_root: value.message_receipt_root.into(),
            height: value.height.into(),
            prev_root: value.prev_root.into(),
            time: value.time.0,
            application_hash: value.application_hash.into(),
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
            chain_config_hash: value.chain_config_hash.into(),
            coins_root: value.coins_root.into(),
            contracts_root: value.contracts_root.into(),
            messages_root: value.messages_root.into(),
        }
    }
}

impl From<schema::block::PoAConsensus> for PoAConsensus {
    fn from(value: schema::block::PoAConsensus) -> Self {
        let bytes: [u8; 64] = value.signature.0 .0.into();
        Self {
            signature: Signature::from_bytes(bytes),
        }
    }
}

impl From<schema::block::Block> for Block {
    fn from(value: schema::block::Block) -> Self {
        let transactions = value
            .transactions
            .iter()
            .map(|tx| tx.id.clone())
            .map(Into::into)
            .collect::<Vec<TransactionId>>();
        let block_producer = value.block_producer();
        Self {
            id: value.id.into(),
            header: value.header.into(),
            consensus: value.consensus.into(),
            transactions,
            block_producer,
        }
    }
}

impl From<schema::block::BlockConnection> for PaginatedResult<Block, String> {
    fn from(conn: schema::block::BlockConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}
