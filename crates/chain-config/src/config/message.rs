use crate::{
    serialization::{HexNumber, HexType},
    GenesisCommitment,
};
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_asm::Word,
    fuel_crypto::Hasher,
    fuel_types::{Address, Nonce},
};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct MessageConfig {
    #[serde_as(as = "HexType")]
    pub sender: Address,
    #[serde_as(as = "HexType")]
    pub recipient: Address,
    #[serde_as(as = "HexType")]
    pub nonce: Nonce,
    #[serde_as(as = "HexNumber")]
    pub amount: Word,
    #[serde_as(as = "HexType")]
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    #[serde_as(as = "HexNumber")]
    pub da_height: DaBlockHeight,
}

#[cfg(all(test, feature = "random"))]
impl MessageConfig {
    pub fn random(rng: &mut impl rand::Rng) -> Self {
        MessageConfig {
            sender: Address::new(super::random_bytes_32(rng)),
            recipient: Address::new(super::random_bytes_32(rng)),
            nonce: Nonce::new(super::random_bytes_32(rng)),
            amount: rng.gen(),
            data: (super::random_bytes_32(rng)).to_vec(),
            da_height: DaBlockHeight(rng.gen()),
        }
    }
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
        }
    }
}

impl GenesisCommitment for Message {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let Self {
            sender,
            recipient,
            nonce,
            amount,
            data,
            da_height,
        } = self;

        let message_hash = *Hasher::default()
            .chain(sender)
            .chain(recipient)
            .chain(nonce)
            .chain(amount.to_be_bytes())
            .chain(data.as_slice())
            .chain(da_height.to_be_bytes())
            .finalize();
        Ok(message_hash)
    }
}
