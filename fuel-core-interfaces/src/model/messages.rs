use super::BlockHeight;
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
    pub fuel_block_spend: Option<BlockHeight>,
}

impl DaMessage {
    fn id(&self) -> Bytes32 {
        let mut hasher = Hasher::default();
        hasher.input(self.sender);
        hasher.input(self.recipient);
        hasher.input(self.owner);
        hasher.input(self.nonce.to_be_bytes());
        hasher.input(self.amount.to_be_bytes());
        hasher.input(&self.data);
        hasher.digest()
    }

    pub fn check(self) -> DaMessageChecked {
        let id = self.id();
        DaMessageChecked { message: self, id }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DaMessageChecked {
    message: DaMessage,
    id: Bytes32,
}

impl DaMessageChecked {
    pub fn id(&self) -> &Bytes32 {
        &self.id
    }

    pub fn unlock(self) -> DaMessage {
        self.message
    }
}

impl AsRef<DaMessage> for DaMessageChecked {
    fn as_ref(&self) -> &DaMessage {
        &self.message
    }
}

impl Deref for DaMessageChecked {
    type Target = DaMessage;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}
