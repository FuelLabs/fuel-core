//! Higher level domain types

use crate::entities::relayer::message::MessageV1;
use coins::message_coin::MessageCoin;
pub use relayer::{
    message::Message,
    transaction::RelayedTransaction,
};

#[cfg(feature = "alloc")]
use alloc::vec;

pub mod coins;
pub mod contract;
pub mod relayer;

impl TryFrom<Message> for MessageCoin {
    type Error = anyhow::Error;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let sender = *message.sender();
        let recipient = *message.recipient();
        let nonce = *message.nonce();
        let amount = message.amount();
        let data = message.data();
        let da_height = message.da_height();

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

        MessageV1 {
            sender,
            recipient,
            nonce,
            amount,
            data: vec![],
            da_height,
        }
        .into()
    }
}
