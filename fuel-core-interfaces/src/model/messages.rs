use super::BlockHeight;
use crate::model::DaBlockHeight;
use core::ops::Deref;
use fuel_crypto::Hasher;
use fuel_types::{Address, Bytes32, Word};

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaMessage {
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

impl DaMessage {
    pub fn id(&self) -> Bytes32 {
        let mut hasher = Hasher::default();
        hasher.input(self.sender);
        hasher.input(self.recipient);
        hasher.input(self.owner);
        hasher.input(self.nonce.to_be_bytes());
        hasher.input(self.amount.to_be_bytes());
        hasher.input(&self.data);
        hasher.digest()
    }

    pub fn check(self) -> CheckedDaMessage {
        let id = self.id();
        CheckedDaMessage { message: self, id }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckedDaMessage {
    message: DaMessage,
    id: Bytes32,
}

impl CheckedDaMessage {
    pub fn id(&self) -> &Bytes32 {
        &self.id
    }
}

impl From<CheckedDaMessage> for DaMessage {
    fn from(checked_message: CheckedDaMessage) -> Self {
        checked_message.message
    }
}

impl AsRef<DaMessage> for CheckedDaMessage {
    fn as_ref(&self) -> &DaMessage {
        &self.message
    }
}

impl Deref for CheckedDaMessage {
    type Target = DaMessage;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}
