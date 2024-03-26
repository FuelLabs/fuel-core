use crate::{
    serialization::HexIfHumanReadable,
    GenesisCommitment,
    TableEntry,
};
use fuel_core_storage::{
    tables::Messages,
    MerkleRoot,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::relayer::message::{
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

impl From<TableEntry<Messages>> for MessageConfig {
    fn from(value: TableEntry<Messages>) -> Self {
        Self {
            sender: *value.value.sender(),
            recipient: *value.value.recipient(),
            nonce: *value.value.nonce(),
            amount: value.value.amount(),
            data: value.value.data().to_vec(),
            da_height: value.value.da_height(),
        }
    }
}

impl From<MessageConfig> for TableEntry<Messages> {
    fn from(value: MessageConfig) -> Self {
        TableEntry {
            key: value.nonce,
            value: Message::V1(MessageV1 {
                sender: value.sender,
                recipient: value.recipient,
                nonce: value.nonce,
                amount: value.amount,
                data: value.data,
                da_height: value.da_height,
            }),
        }
    }
}

#[cfg(feature = "test-helpers")]
impl crate::Randomize for MessageConfig {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self {
            sender: crate::Randomize::randomize(&mut rng),
            recipient: crate::Randomize::randomize(&mut rng),
            nonce: crate::Randomize::randomize(&mut rng),
            amount: crate::Randomize::randomize(&mut rng),
            data: fuel_core_types::fuel_types::Bytes32::randomize(&mut rng).to_vec(),
            da_height: crate::Randomize::randomize(&mut rng),
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
