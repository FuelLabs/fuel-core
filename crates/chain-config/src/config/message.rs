use crate::GenesisCommitment;
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::Message,
    fuel_asm::Word,
    fuel_crypto::Hasher,
    fuel_types::{
        Address,
        Nonce,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct MessageConfig {
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub amount: Word,
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
}

#[cfg(all(test, feature = "random"))]
impl crate::Randomize for MessageConfig {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self {
            sender: Address::new(super::random_bytes_32(&mut rng)),
            recipient: Address::new(super::random_bytes_32(&mut rng)),
            nonce: Nonce::new(super::random_bytes_32(&mut rng)),
            amount: rng.gen(),
            data: (super::random_bytes_32(&mut rng)).to_vec(),
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
