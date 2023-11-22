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

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Message {
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

impl Message {
    /// Returns the id of the message
    pub fn id(&self) -> &Nonce {
        &self.nonce
    }

    /// Computed message id
    pub fn message_id(&self) -> MessageId {
        compute_message_id(
            &self.sender,
            &self.recipient,
            &self.nonce,
            self.amount,
            &self.data,
        )
    }

    /// Verifies the integrity of the message.
    ///
    /// Returns `None`, if the `input` is not a message.
    /// Otherwise returns the result of the field comparison.
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
                let expected_data = if self.data.is_empty() {
                    None
                } else {
                    Some(self.data.as_slice())
                };
                Some(
                    &self.sender == sender
                        && &self.recipient == recipient
                        && &self.nonce == nonce
                        && &self.amount == amount
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
