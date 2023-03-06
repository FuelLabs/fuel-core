use crate::{
    serialization::{
        HexNumber,
        HexType,
    },
    GenesisCommitment,
};
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        message::CompressedMessage,
        Nonce,
    },
    fuel_asm::Word,
    fuel_crypto::Hasher,
    fuel_types::Address,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct MessageConfig {
    #[serde_as(as = "HexType")]
    pub sender: Address,
    #[serde_as(as = "HexType")]
    pub recipient: Address,
    #[serde_as(as = "HexNumber")]
    pub nonce: Nonce,
    #[serde_as(as = "HexNumber")]
    pub amount: Word,
    #[serde_as(as = "HexType")]
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    #[serde_as(as = "HexNumber")]
    pub da_height: DaBlockHeight,
}

impl From<MessageConfig> for CompressedMessage {
    fn from(msg: MessageConfig) -> Self {
        CompressedMessage {
            sender: msg.sender,
            recipient: msg.recipient,
            nonce: msg.nonce,
            amount: msg.amount,
            data: msg.data,
            da_height: msg.da_height,
        }
    }
}

impl GenesisCommitment for CompressedMessage {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let message_hash = *Hasher::default()
            .chain(self.sender)
            .chain(self.recipient)
            .chain(self.nonce.to_be_bytes())
            .chain(self.amount.to_be_bytes())
            .chain(self.data.as_slice())
            .chain(self.da_height.to_be_bytes())
            .finalize();
        Ok(message_hash)
    }
}
