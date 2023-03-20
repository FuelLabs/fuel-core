//! Higher level domain types

use crate::fuel_asm::Word;
#[cfg(feature = "random")]
use crate::fuel_crypto::rand::{
    distributions::{
        Distribution,
        Standard,
    },
    Rng,
};
use coins::message_coin::{
    CompressedMessageCoin,
    MessageCoin,
};
use message::Message;

pub mod coins;
pub mod contract;
pub mod message;

/// The nonce is a unique identifier for the message.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Add,
    derive_more::Sub,
    derive_more::Display,
    derive_more::Into,
    derive_more::From,
    derive_more::Deref,
    derive_more::LowerHex,
    derive_more::UpperHex,
)]
pub struct Nonce(Word);

impl Nonce {
    /// The length of the serialized nonce.
    pub const LEN: usize = 8;
}

#[cfg(feature = "random")]
impl Distribution<Nonce> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Nonce {
        Nonce(rng.gen())
    }
}

impl TryFrom<Message> for CompressedMessageCoin {
    type Error = anyhow::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let Message {
            sender,
            recipient,
            nonce,
            amount,
            data,
            da_height,
        } = message;

        if !data.is_empty() {
            return Err(anyhow::anyhow!(
                "The data is not empty, impossible to convert into the `CompressedMessageCoin`"
            ))
        }

        let coin = CompressedMessageCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        };

        Ok(coin)
    }
}

impl From<CompressedMessageCoin> for Message {
    fn from(coin: CompressedMessageCoin) -> Self {
        let CompressedMessageCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        } = coin;

        Message {
            sender,
            recipient,
            nonce,
            amount,
            data: vec![],
            da_height,
        }
    }
}

impl TryFrom<Message> for MessageCoin {
    type Error = anyhow::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let Message {
            sender,
            recipient,
            nonce,
            amount,
            data,
            da_height,
        } = message;

        if !data.is_empty() {
            return Err(anyhow::anyhow!(
                "The data is not empty, impossible to convert into the `MessageCoin`"
            ))
        }

        let coin = MessageCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        };

        Ok(coin)
    }
}

impl From<MessageCoin> for Message {
    fn from(coin: MessageCoin) -> Self {
        let MessageCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        } = coin;

        Message {
            sender,
            recipient,
            nonce,
            amount,
            data: vec![],
            da_height,
        }
    }
}
