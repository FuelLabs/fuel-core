use crate::{
    serialization::HexIfHumanReadable,
    GenesisCommitment,
};
use fuel_core_storage::MerkleRoot;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::{
        Message,
        MessageV1,
    },
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
use serde_with::serde_as;

#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct MessageConfig {
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub amount: Word,
    #[serde_as(as = "HexIfHumanReadable")]
    pub data: Vec<u8>,
    /// The block height from the parent da layer that originated this message
    pub da_height: DaBlockHeight,
}

#[cfg(all(test, feature = "random", feature = "std"))]
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
        MessageV1 {
            sender: msg.sender,
            recipient: msg.recipient,
            nonce: msg.nonce,
            amount: msg.amount,
            data: msg.data,
            da_height: msg.da_height,
        }
        .into()
    }
}

impl GenesisCommitment for Message {
    fn root(&self) -> anyhow::Result<MerkleRoot> {
        let sender = self.sender();
        let recipient = self.recipient();
        let nonce = self.nonce();
        let amount = self.amount();
        let data = self.data();
        let da_height = self.da_height();

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
