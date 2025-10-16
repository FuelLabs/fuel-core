use crate::{
    abi,
    config,
};
use alloy_rpc_types_eth::Log;
use alloy_sol_types::SolEvent;
use anyhow::anyhow;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        RelayedTransaction,
        relayer::{
            message::{
                Message,
                MessageV1,
            },
            transaction::RelayedTransactionV1,
        },
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
    pub nonce: Nonce,
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
            nonce: transaction.nonce,
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
        if log.topics().is_empty() {
            return Err(anyhow!("Topic list is empty"));
        }

        let log = match log.topics()[0] {
            n if n == *config::ETH_LOG_MESSAGE => parse_message_to_event(log)?,
            n if n == *config::ETH_FORCED_TX => parse_forced_tx_to_event(log)?,
            _ => Self::Ignored,
        };

        Ok(log)
    }
}

fn parse_message_to_event(log: &Log) -> anyhow::Result<EthEventLog> {
    // event has 3 indexed fields, so it should have 4 topics
    if log.topics().len() != 4 {
        return Err(anyhow!(
            "Malformed topics for Message. expected: 4, found: {}",
            log.topics().len()
        ));
    }

    let message =
        abi::bridge::MessageSent::decode_log(&log.inner).map_err(anyhow::Error::msg)?;
    let amount = message.amount;
    let data = message.data.data.to_vec();
    let nonce = Nonce::new(message.nonce.to_be_bytes());
    let recipient = Address::from(message.recipient.0);
    let sender = Address::from(message.sender.0);

    Ok(EthEventLog::Message(MessageLog {
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
                .ok_or(anyhow!("Log missing block height"))?,
        ),
    }))
}

fn parse_forced_tx_to_event(log: &Log) -> anyhow::Result<EthEventLog> {
    // event has one indexed field, so there are 2 topics
    let block_number = log
        .block_number
        .ok_or(anyhow!("Log missing block height"))?;

    if log.topics().len() != 2 {
        return Err(anyhow!(
            "Malformed topics for transaction. expected: 2, found: {}",
            log.topics().len()
        ));
    }

    let event =
        abi::bridge::Transaction::decode_log(&log.inner).map_err(anyhow::Error::msg)?;

    let nonce = Nonce::new(event.nonce.to_be_bytes());
    let max_gas = event.max_gas;
    let serialized_transaction = event.canonically_serialized_tx.to_vec();

    Ok(EthEventLog::Transaction(TransactionLog {
        nonce,
        max_gas,
        serialized_transaction,
        // Safety: logs without block numbers are rejected by
        // FinalizationQueue::append_eth_log before the conversion to EthEventLog happens.
        // If block_number is none, that means the log is pending.
        da_height: DaBlockHeight::from(block_number),
    }))
}
