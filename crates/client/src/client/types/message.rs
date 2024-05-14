use crate::client::{
    schema,
    schema::ConversionError,
    types::{
        block::Header,
        primitives::{
            Address,
            Bytes,
            Nonce,
        },
        MerkleProof,
    },
    PaginatedResult,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub amount: u64,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub data: Bytes,
    pub da_height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageProof {
    /// Proof that message is contained within the provided block header.
    pub message_proof: MerkleProof,
    /// Proof that the provided block header is contained within the blockchain history.
    pub block_proof: MerkleProof,
    /// The previous fuel block header that contains the message. Message block height <
    /// commit block height.
    pub message_block_header: Header,
    /// The consensus header associated with the finalized commit being used
    /// as the root of the block proof.
    pub commit_block_header: Header,
    /// The messages sender address.
    pub sender: Address,
    /// The messages recipient address.
    pub recipient: Address,
    /// The nonce from the message.
    pub nonce: Nonce,
    /// The amount from the message.
    pub amount: u64,
    /// The data from the message.
    pub data: Bytes,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MessageStatus {
    Unspent,
    Spent,
    NotFound,
}

impl From<schema::message::MessageStatus> for MessageStatus {
    fn from(value: schema::message::MessageStatus) -> Self {
        match value.state {
            schema::message::MessageState::Unspent => Self::Unspent,
            schema::message::MessageState::Spent => Self::Spent,
            schema::message::MessageState::NotFound => Self::NotFound,
        }
    }
}

// GraphQL Translation

impl From<schema::message::Message> for Message {
    fn from(value: schema::message::Message) -> Self {
        Self {
            amount: value.amount.into(),
            sender: value.sender.into(),
            recipient: value.recipient.into(),
            nonce: value.nonce.into(),
            data: value.data.into(),
            da_height: value.da_height.into(),
        }
    }
}

impl From<schema::message::MessageConnection> for PaginatedResult<Message, String> {
    fn from(conn: schema::message::MessageConnection) -> Self {
        PaginatedResult {
            cursor: conn.page_info.end_cursor,
            has_next_page: conn.page_info.has_next_page,
            has_previous_page: conn.page_info.has_previous_page,
            results: conn.edges.into_iter().map(|e| e.node.into()).collect(),
        }
    }
}

impl TryFrom<schema::message::MessageProof> for MessageProof {
    type Error = ConversionError;

    fn try_from(value: schema::message::MessageProof) -> Result<Self, Self::Error> {
        Ok(Self {
            message_proof: value.message_proof.into(),
            block_proof: value.block_proof.into(),
            message_block_header: value.message_block_header.try_into()?,
            commit_block_header: value.commit_block_header.try_into()?,
            sender: value.sender.into(),
            recipient: value.recipient.into(),
            nonce: value.nonce.into(),
            amount: value.amount.into(),
            data: value.data.into(),
        })
    }
}
