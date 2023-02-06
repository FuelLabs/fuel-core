//! Message

use crate::{
    blockchain::{
        header::BlockHeader,
        primitives::DaBlockHeight,
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
pub struct CompressedMessage {
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
}

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
    /// Whether a message has been spent or not
    pub status: MessageStatus,
}

impl CompressedMessage {
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

    /// Decompress the message
    pub fn decompress(self, status: MessageStatus) -> Message {
        Message {
            status,
            sender: self.sender,
            recipient: self.recipient,
            nonce: self.nonce,
            amount: self.amount,
            data: self.data,
            da_height: self.da_height,
        }
    }
}

impl Message {
    /// Compress the message
    pub fn compress(self) -> CompressedMessage {
        CompressedMessage {
            sender: self.sender,
            recipient: self.recipient,
            nonce: self.nonce,
            amount: self.amount,
            data: self.data,
            da_height: self.da_height,
        }
    }

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
}

/// A message associated with precomputed id
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckedMessage {
    message: CompressedMessage,
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

/// Whether the message has been spent or not
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq, Default)]
#[repr(u8)]
pub enum MessageStatus {
    #[default]
    /// Message has not been spent
    Unspent,
    /// Message has been spent
    Spent,
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
    /// Returned computed message id.
    pub fn id(&self) -> &MessageId {
        &self.id
    }

    /// Returns the message.
    pub fn message(&self) -> &CompressedMessage {
        &self.message
    }

    /// Unpacks inner values of the checked message.
    pub fn unpack(self) -> (MessageId, CompressedMessage) {
        (self.id, self.message)
    }
}

impl From<CheckedMessage> for CompressedMessage {
    fn from(checked_message: CheckedMessage) -> Self {
        checked_message.message
    }
}

impl AsRef<CompressedMessage> for CheckedMessage {
    fn as_ref(&self) -> &CompressedMessage {
        &self.message
    }
}

impl Deref for CheckedMessage {
    type Target = CompressedMessage;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}
