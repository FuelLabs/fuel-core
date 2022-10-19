use super::{
    BlockHeight,
    FuelBlockDb,
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

pub struct OutputProof {
    /// The merkle root of the message proof.
    pub root: Bytes32,
    /// The proof set of the message proof.
    pub proof_set: Vec<Bytes32>,
    /// The out put message.
    pub message: Message,
    /// The signature of the fuel block.
    pub signature: fuel_crypto::Signature,
    /// The fuel block that contains the message.
    pub block: FuelBlockDb,
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
