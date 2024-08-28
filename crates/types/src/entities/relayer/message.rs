//! Message

use crate::{
    blockchain::{
        header::BlockHeader,
        primitives::DaBlockHeight,
    },
    fuel_merkle::common::ProofSet,
    fuel_tx::{
        input::message::{
            compute_message_id,
            MessageCoinPredicate,
            MessageCoinSigned,
            MessageDataPredicate,
            MessageDataSigned,
        },
        Input,
    },
    fuel_types::{
        Address,
        MessageId,
        Nonce,
        Word,
    },
};

#[cfg(feature = "alloc")]
use alloc::vec::Vec;

/// Message sent from DA layer to fuel by relayer bridge.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Message {
    /// Message Version 1
    V1(MessageV1),
}

#[cfg(any(test, feature = "test-helpers"))]
impl Default for Message {
    fn default() -> Self {
        Self::V1(Default::default())
    }
}

/// The V1 version of the message from the DA layer.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MessageV1 {
    /// Account that sent the message from the da layer
    pub sender: Address,
    /// Fuel account receiving the message
    pub recipient: Address,
    /// Nonce must be unique. It's used to prevent replay attacks
    pub nonce: Nonce,
    /// The amount of the base asset of Fuel chain sent along this message
    pub amount: Word,
    /// Arbitrary message data
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
}

impl From<MessageV1> for Message {
    fn from(value: MessageV1) -> Self {
        Self::V1(value)
    }
}

impl Message {
    /// Get the message sender
    pub fn sender(&self) -> &Address {
        match self {
            Message::V1(message) => &message.sender,
        }
    }

    /// Set the message sender
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_sender(&mut self, sender: Address) {
        match self {
            Message::V1(message) => message.sender = sender,
        }
    }

    /// Get the message recipient
    pub fn recipient(&self) -> &Address {
        match self {
            Message::V1(message) => &message.recipient,
        }
    }

    /// Set the message recipient
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_recipient(&mut self, recipient: Address) {
        match self {
            Message::V1(message) => message.recipient = recipient,
        }
    }

    /// Get the message nonce
    pub fn nonce(&self) -> &Nonce {
        match self {
            Message::V1(message) => &message.nonce,
        }
    }

    /// Set the message nonce
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_nonce(&mut self, nonce: Nonce) {
        match self {
            Message::V1(message) => message.nonce = nonce,
        }
    }

    /// Get the message amount
    pub fn amount(&self) -> Word {
        match self {
            Message::V1(message) => message.amount,
        }
    }

    /// Set the message amount
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_amount(&mut self, amount: Word) {
        match self {
            Message::V1(message) => message.amount = amount,
        }
    }

    /// Get the message data
    pub fn data(&self) -> &Vec<u8> {
        match self {
            Message::V1(message) => &message.data,
        }
    }

    /// Set the message data
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_data(&mut self, data: Vec<u8>) {
        match self {
            Message::V1(message) => message.data = data,
        }
    }

    /// Get the message DA height
    pub fn da_height(&self) -> DaBlockHeight {
        match self {
            Message::V1(message) => message.da_height,
        }
    }

    /// Set the message DA height
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn set_da_height(&mut self, da_height: DaBlockHeight) {
        match self {
            Message::V1(message) => message.da_height = da_height,
        }
    }

    /// Returns the id of the message
    pub fn id(&self) -> &Nonce {
        match self {
            Message::V1(message) => &message.nonce,
        }
    }

    /// Computed message id
    pub fn message_id(&self) -> MessageId {
        compute_message_id(
            self.sender(),
            self.recipient(),
            self.nonce(),
            self.amount(),
            self.data(),
        )
    }

    /// Verifies the integrity of the message.
    ///
    /// Returns `None`, if the `input` is not a message.
    /// Otherwise, returns the result of the field comparison.
    pub fn matches_input(&self, input: &Input) -> Option<bool> {
        match input {
            Input::MessageDataSigned(MessageDataSigned {
                sender,
                recipient,
                nonce,
                amount,
                ..
            })
            | Input::MessageDataPredicate(MessageDataPredicate {
                sender,
                recipient,
                nonce,
                amount,
                ..
            })
            | Input::MessageCoinSigned(MessageCoinSigned {
                sender,
                recipient,
                nonce,
                amount,
                ..
            })
            | Input::MessageCoinPredicate(MessageCoinPredicate {
                sender,
                recipient,
                nonce,
                amount,
                ..
            }) => {
                let expected_data = if self.data().is_empty() {
                    None
                } else {
                    Some(self.data().as_slice())
                };
                Some(
                    self.sender() == sender
                        && self.recipient() == recipient
                        && self.nonce() == nonce
                        && &self.amount() == amount
                        && expected_data == input.input_data(),
                )
            }
            _ => None,
        }
    }
}

/// Type containing merkle proof data.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The proof set.
    pub proof_set: ProofSet,
    /// The proof index.
    pub proof_index: u64,
}

/// Proves to da layer that this message was included in a Fuel block.
pub struct MessageProof {
    /// Proof that message is contained within the provided block header.
    pub message_proof: MerkleProof,
    /// Proof that the provided block header is contained within the blockchain history.
    pub block_proof: MerkleProof,
    /// The previous fuel block header that contains the message. Message block height <
    /// commit block height.
    pub message_block_header: BlockHeader,
    /// The consensus header associated with the finalized commit being used
    /// as the root of the block proof.
    pub commit_block_header: BlockHeader,

    /// The messages sender address.
    pub sender: Address,
    /// The messages recipient address.
    pub recipient: Address,
    /// The nonce from the message.
    pub nonce: Nonce,
    /// The amount from the message.
    pub amount: Word,
    /// The data from the message.
    pub data: Vec<u8>,
}

impl MessageProof {
    /// Compute message id from the proof
    pub fn message_id(&self) -> MessageId {
        compute_message_id(
            &self.sender,
            &self.recipient,
            &self.nonce,
            self.amount,
            &self.data,
        )
    }
}

/// Represents the status of a message
pub struct MessageStatus {
    /// The message state
    pub state: MessageState,
}

impl MessageStatus {
    /// Constructor for `MessageStatus` that fills with `Unspent` state
    pub fn unspent() -> Self {
        Self {
            state: MessageState::Unspent,
        }
    }

    /// Constructor for `MessageStatus` that fills with `Spent` state
    pub fn spent() -> Self {
        Self {
            state: MessageState::Spent,
        }
    }

    /// Constructor for `MessageStatus` that fills with `Unknown` state
    pub fn not_found() -> Self {
        Self {
            state: MessageState::NotFound,
        }
    }
}

/// The possible states a Message can be in
pub enum MessageState {
    /// Message is still unspent
    Unspent,
    /// Message has already been spent
    Spent,
    /// There is no record of this Message
    NotFound,
}
