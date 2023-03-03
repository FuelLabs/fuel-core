//! Higher level domain types

use coins::{
    deposit_coin::{
        CompressedDepositCoin,
        DepositCoin,
    },
    CoinStatus,
};
use message::{
    CheckedMessage,
    CompressedMessage,
    Message,
    MessageStatus,
};

pub mod coins;
pub mod message;
pub mod resource;

impl TryFrom<CheckedMessage> for CompressedDepositCoin {
    type Error = anyhow::Error;

    fn try_from(checked_message: CheckedMessage) -> Result<Self, Self::Error> {
        let (_, message) = checked_message.unpack();
        let CompressedMessage {
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

impl From<CompressedDepositCoin> for CompressedMessage {
    fn from(coin: CompressedDepositCoin) -> Self {
        let CompressedDepositCoin {
            sender,
            recipient,
            nonce,
            amount,
            da_height,
        } = coin;

        CompressedMessage {
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
            status,
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
            status: status.into(),
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
            status,
        } = coin;

        Message {
            sender,
            recipient,
            nonce,
            amount,
            data: vec![],
            da_height,
            status: status.into(),
        }
    }
}

impl From<MessageStatus> for CoinStatus {
    fn from(status: MessageStatus) -> Self {
        if let MessageStatus::Spent = status {
            CoinStatus::Spent
        } else {
            CoinStatus::Unspent
        }
    }
}

impl From<CoinStatus> for MessageStatus {
    fn from(status: CoinStatus) -> Self {
        if let CoinStatus::Spent = status {
            MessageStatus::Spent
        } else {
            MessageStatus::Unspent
        }
    }
}
