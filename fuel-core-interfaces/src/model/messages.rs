use super::{
    BlockHeight,
    FuelBlockHeader,
};
use crate::{
    common::{
        fuel_types::{
            Address,
            MessageId,
            Word,
        },
        fuel_vm::prelude::Input,
    },
    model::DaBlockHeight,
};
use core::ops::Deref;
use fuel_vm::{
    fuel_crypto,
    fuel_tx::Bytes32,
    prelude::Output,
};

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Message {
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Word,
    pub amount: Word,
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
    pub fuel_block_spend: Option<BlockHeight>,
}

impl Message {
    pub fn id(&self) -> MessageId {
        Input::compute_message_id(
            &self.sender,
            &self.recipient,
            self.nonce,
            self.amount,
            &self.data,
        )
    }

    pub fn check(self) -> CheckedMessage {
        let id = self.id();
        CheckedMessage { message: self, id }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckedMessage {
    message: Message,
    id: MessageId,
}

pub struct MessageProof {
    /// The proof set of the message proof.
    pub proof_set: Vec<Bytes32>,
    /// The index used to generate this proof.
    pub proof_index: u64,
    /// The signature of the fuel block.
    pub signature: fuel_crypto::Signature,
    /// The fuel block header that contains the message.
    pub header: FuelBlockHeader,
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
