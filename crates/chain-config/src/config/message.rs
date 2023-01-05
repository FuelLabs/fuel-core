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
    entities::message::Message,
    fuel_asm::Word,
    fuel_types::Address,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::{
    serde_as,
    skip_serializing_none,
};

#[skip_serializing_none]
#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct MessageConfig {
    #[serde_as(as = "HexType")]
    pub sender: Address,
    #[serde_as(as = "HexType")]
    pub recipient: Address,
    #[serde_as(as = "HexNumber")]
    pub nonce: Word,
    #[serde_as(as = "HexNumber")]
    pub amount: Word,
    #[serde_as(as = "HexType")]
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    #[serde_as(as = "HexNumber")]
    pub da_height: DaBlockHeight,
}

impl From<MessageConfig> for Message {
    fn from(msg: MessageConfig) -> Self {
        Message {
            sender: msg.sender,
            recipient: msg.recipient,
            nonce: msg.nonce,
            amount: msg.amount,
            data: msg.data,
            da_height: msg.da_height,
            fuel_block_spend: None,
        }
    }
}

impl GenesisCommitment for Message {
    fn root(&mut self) -> anyhow::Result<MerkleRoot> {
        Ok(self.id().into())
    }
}
