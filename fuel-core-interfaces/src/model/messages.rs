use super::BlockHeight;
use crate::model::DaBlockHeight;
use core::ops::Deref;
use fuel_crypto::Hasher;
use fuel_types::{Address, MessageId, Word};

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Message {
    pub sender: Address,
    pub recipient: Address,
    pub owner: Address,
    pub nonce: Word,
    pub amount: Word,
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
    pub fuel_block_spend: Option<BlockHeight>,
}

impl Message {
    pub fn id(&self) -> MessageId {
        let mut hasher = Hasher::default();
        hasher.input(self.sender);
        hasher.input(self.recipient);
        hasher.input(self.nonce.to_be_bytes());
        hasher.input(self.owner);
        hasher.input(self.amount.to_be_bytes());
        hasher.input(&self.data);
        MessageId::from(*hasher.digest())
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
