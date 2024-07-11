use crate::client::{
    schema,
    schema::ConversionError,
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

use crate::client::schema::block::{
    BlockVersion,
    HeaderVersion,
};
use tai64::Tai64;

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub id: BlockId,
    pub da_height: u64,
    pub consensus_parameters_version: u32,
    pub state_transition_bytecode_version: u32,
    pub transactions_count: u16,
    pub message_receipt_count: u32,
    pub transactions_root: MerkleRoot,
    pub message_outbox_root: MerkleRoot,
    pub event_inbox_root: MerkleRoot,
    pub height: u32,
    pub prev_root: MerkleRoot,
    pub time: Tai64,
    pub application_hash: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Consensus {
    Genesis(Genesis),
    PoAConsensus(PoAConsensus),
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Genesis {
    pub chain_config_hash: Hash,
    pub coins_root: MerkleRoot,
    pub contracts_root: MerkleRoot,
    pub messages_root: MerkleRoot,
    pub transactions_root: MerkleRoot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoAConsensus {
    pub signature: Signature,
}

// GraphQL Translation

impl TryFrom<schema::block::Header> for Header {
    type Error = ConversionError;

    fn try_from(value: schema::block::Header) -> Result<Self, Self::Error> {
        match value.version {
            HeaderVersion::V1 => Ok(Self {
                id: value.id.into(),
                da_height: value.da_height.into(),
                consensus_parameters_version: value.consensus_parameters_version.into(),
                state_transition_bytecode_version: value
                    .state_transition_bytecode_version
                    .into(),
                transactions_count: value.transactions_count.into(),
                message_receipt_count: value.message_receipt_count.into(),
                transactions_root: value.transactions_root.into(),
                message_outbox_root: value.message_outbox_root.into(),
                event_inbox_root: value.event_inbox_root.into(),
                height: value.height.into(),
                prev_root: value.prev_root.into(),
                time: value.time.0,
                application_hash: value.application_hash.into(),
            }),
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
            transactions_root: value.transactions_root.into(),
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

impl TryFrom<schema::block::Block> for Block {
    type Error = ConversionError;

    fn try_from(value: schema::block::Block) -> Result<Self, Self::Error> {
        match value.version {
            BlockVersion::V1 => {
                let block_producer = value.block_producer();
                let id = value.id.into();
                let header = value.header.try_into()?;
                let consensus = value.consensus.into();
                let transactions = value
                    .transaction_ids
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<TransactionId>>();
                Ok(Self {
                    id,
                    header,
                    consensus,
                    transactions,
                    block_producer,
                })
            }
        }
    }
}

impl TryFrom<schema::block::BlockConnection> for PaginatedResult<Block, String> {
    type Error = ConversionError;

    fn try_from(conn: schema::block::BlockConnection) -> Result<Self, Self::Error> {
        Ok(PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn
                .edges
                .into_iter()
                .map(|e| e.node.try_into())
                .collect::<Result<_, _>>()?,
        })
    }
}
