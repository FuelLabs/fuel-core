use crate::{
    abi,
    config,
};
use anyhow::anyhow;
use ethers_contract::EthEvent;
use ethers_core::{
    abi::RawLog,
    types::Log,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        relayer::{
            message::{
                Message,
                MessageV1,
            },
            transaction::RelayedTransactionV1,
        },
        RelayedTransaction,
    },
    fuel_types::{
        Address,
        Nonce,
        Word,
    },
};

/// Bridge message send from da to fuel network.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MessageLog {
    pub sender: Address,
    pub recipient: Address,
    pub nonce: Nonce,
    pub amount: Word,
    pub data: Vec<u8>,
    pub da_height: DaBlockHeight,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TransactionLog {
    pub max_gas: u64,
    pub serialized_transaction: Vec<u8>,
    pub da_height: DaBlockHeight,
}

impl From<&MessageLog> for Message {
    fn from(message: &MessageLog) -> Self {
        MessageV1 {
            sender: message.sender,
            recipient: message.recipient,
            nonce: message.nonce,
            amount: message.amount,
            data: message.data.clone(),
            da_height: message.da_height,
        }
        .into()
    }
}

impl From<TransactionLog> for RelayedTransaction {
    fn from(transaction: TransactionLog) -> Self {
        RelayedTransactionV1 {
            max_gas: transaction.max_gas,
            serialized_transaction: transaction.serialized_transaction,
            da_height: transaction.da_height,
        }
        .into()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EthEventLog {
    // Bridge message from da side
    Message(MessageLog),
    Transaction(TransactionLog),
    Ignored,
}

impl TryFrom<&Log> for EthEventLog {
    type Error = anyhow::Error;

    fn try_from(log: &Log) -> Result<Self, Self::Error> {
        if log.topics.is_empty() {
            return Err(anyhow!("Topic list is empty"))
        }

        let log = match log.topics[0] {
            n if n == *config::ETH_LOG_MESSAGE => {
                // event has 3 indexed fields, so it should have 4 topics
                if log.topics.len() != 4 {
                    return Err(anyhow!("Malformed topics for Message"))
                }

                let raw_log = RawLog {
                    topics: log.topics.clone(),
                    data: log.data.to_vec(),
                };

                let message = abi::bridge::MessageSentFilter::decode_log(&raw_log)
                    .map_err(anyhow::Error::msg)?;
                let amount = message.amount;
                let data = message.data.to_vec();
                let mut nonce = Nonce::zeroed();
                message.nonce.to_big_endian(nonce.as_mut());
                let recipient = Address::from(message.recipient);
                let sender = Address::from(message.sender);

                Self::Message(MessageLog {
                    amount,
                    data,
                    nonce,
                    sender,
                    recipient,
                    // Safety: logs without block numbers are rejected by
                    // FinalizationQueue::append_eth_log before the conversion to EthEventLog happens.
                    // If block_number is none, that means the log is pending.
                    da_height: DaBlockHeight::from(
                        log.block_number
                            .ok_or(anyhow!("Log missing block height"))?
                            .as_u64(),
                    ),
                })
            }
            n if n == *config::ETH_FORCED_TX => {
                // event has no indexed fields, so there is only 1 topic
                if log.topics.len() != 1 {
                    return Err(anyhow!("Malformed topics for forced Transaction"))
                }

                let raw_log = RawLog {
                    topics: log.topics.clone(),
                    data: log.data.to_vec(),
                };

                let event = abi::bridge::TransactionFilter::decode_log(&raw_log)
                    .map_err(anyhow::Error::msg)?;

                let max_gas = event.max_gas;
                let serialized_transaction = event.canonically_serialized_tx;

                Self::Transaction(TransactionLog {
                    max_gas,
                    serialized_transaction: serialized_transaction.to_vec(),
                    // Safety: logs without block numbers are rejected by
                    // FinalizationQueue::append_eth_log before the conversion to EthEventLog happens.
                    // If block_number is none, that means the log is pending.
                    da_height: DaBlockHeight::from(
                        log.block_number
                            .ok_or(anyhow!("Log missing block height"))?
                            .as_u64(),
                    ),
                })
            }
            _ => Self::Ignored,
        };

        Ok(log)
    }
}
