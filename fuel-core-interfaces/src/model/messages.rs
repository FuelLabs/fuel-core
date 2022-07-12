use super::BlockHeight;
use core::ops::Deref;
use fuel_crypto::Hasher;
use fuel_types::{Address, Bytes32, Word};

/// Message send from Da layer to fuel by bridge
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaMessage {
    pub sender: Address,
    pub receipient: Address,
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
        hasher.input(self.receipient);
        hasher.input(self.owner);
        hasher.input(self.nonce.to_be_bytes());
        hasher.input(self.amount.to_be_bytes());
        hasher.input(&self.data);
        hasher.digest()
    }

    pub fn lock(self) -> DaMessageLocked {
        let id = self.id();
        DaMessageLocked { message: self, id }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DaMessageLocked {
    message: DaMessage,
    id: Bytes32,
}

impl DaMessageLocked {
    pub fn id(&self) -> &Bytes32 {
        &self.id
    }

    pub fn unlock(self) -> DaMessage {
        self.message
    }
}

impl AsRef<DaMessage> for DaMessageLocked {
    fn as_ref(&self) -> &DaMessage {
        &self.message
    }
}

impl Deref for DaMessageLocked {
    type Target = DaMessage;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}
