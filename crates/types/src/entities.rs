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
use coins::deposit_coin::{
    CompressedDepositCoin,
    DepositCoin,
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

#[cfg(feature = "random")]
impl Distribution<Nonce> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Nonce {
        Nonce(rng.gen())
    }
}

impl TryFrom<Message> for CompressedDepositCoin {
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
                "The data is not empty, impossible to convert into the `CompressedDepositCoin`"
            ))
        }

        let coin = CompressedDepositCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        };

        Ok(coin)
    }
}

impl From<CompressedDepositCoin> for Message {
    fn from(coin: CompressedDepositCoin) -> Self {
        let CompressedDepositCoin {
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

impl TryFrom<Message> for DepositCoin {
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
                "The data is not empty, impossible to convert into the `DepositCoin`"
            ))
        }

        let coin = DepositCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        };

        Ok(coin)
    }
}

impl From<DepositCoin> for Message {
    fn from(coin: DepositCoin) -> Self {
        let DepositCoin {
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
