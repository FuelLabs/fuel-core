//! Message

use crate::{
    blockchain::{
        header::BlockHeader,
        primitives::{
            BlockHeight,
            DaBlockHeight,
        },
    },
    fuel_tx::{
        Input,
        Output,
    },
    fuel_types::{
        Address,
        Bytes32,
        MessageId,
        Word,
    },
};
use core::ops::Deref;

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Message {
    /// Account that sent the message from the da layer
    pub sender: Address,
    /// Fuel account receiving the message
    pub recipient: Address,
    /// Nonce must be unique. It's used to prevent replay attacks
    pub nonce: Word,
    /// The amount of the base asset of Fuel chain sent along this message
    pub amount: Word,
    /// Arbitrary message data
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
    /// When the message was spent in the Fuel chain
    pub fuel_block_spend: Option<BlockHeight>, // TODO: get rid of this
}

impl Message {
    /// Computed message id
    pub fn id(&self) -> MessageId {
        Input::compute_message_id(
            &self.sender,
            &self.recipient,
            self.nonce,
            self.amount,
            &self.data,
        )
    }

    /// Compute checked message
    pub fn check(self) -> CheckedMessage {
        let id = self.id();
        CheckedMessage { message: self, id }
    }
}

/// A message associated with precomputed id
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckedMessage {
    message: Message,
    id: MessageId,
}

/// Proves to da layer that this message was included in a Fuel block
pub struct MessageProof {
    /// The proof set of the message proof.
    pub proof_set: Vec<Bytes32>,
    /// The index used to generate this proof.
    pub proof_index: u64,
    /// The signature of the fuel block.
    pub signature: crate::fuel_crypto::Signature,
    /// The fuel block header that contains the message.
    pub header: BlockHeader,
    /// The messages sender address.
    pub sender: Address,
    /// The messages recipient address.
    pub recipient: Address,
    /// The nonce from the message.
    pub nonce: Bytes32,
    /// The amount from the message.
    pub amount: Word,
    /// The data from the message.
    pub data: Vec<u8>,
}

impl MessageProof {
    /// Compute message id from the proof
    pub fn message_id(&self) -> MessageId {
        Output::message_id(
            &self.sender,
            &self.recipient,
            &self.nonce,
            self.amount,
            &self.data,
        )
    }
}

impl CheckedMessage {
    /// Compute message id from the proof
    pub fn id(&self) -> &MessageId {
        &self.id
    }
}

impl From<CheckedMessage> for Message {
    fn from(checked_message: CheckedMessage) -> Self {
        checked_message.message
    }
}

impl AsRef<Message> for CheckedMessage {
    fn as_ref(&self) -> &Message {
        &self.message
    }
}

impl Deref for CheckedMessage {
    type Target = Message;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}
