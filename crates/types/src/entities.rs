//! Higher level domain types

use coins::message_coin::MessageCoin;
use message::Message;

pub mod coins;
pub mod contract;
pub mod message;

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
