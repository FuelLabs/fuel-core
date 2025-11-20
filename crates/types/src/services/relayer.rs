//! The module contains types related to the relayer service.

use crate::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        Message,
        RelayedTransaction,
    },
    fuel_types::Bytes32,
};
use core::ops::Deref;

/// The event that may come from the relayer.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The message event which was sent to the bridge.
    Message(Message),
    /// A transaction that was forcibly included from L1
    Transaction(RelayedTransaction),
}

impl Event {
    /// Returns the da height when event happened.
    pub fn da_height(&self) -> DaBlockHeight {
        match self {
            Event::Message(message) => message.da_height(),
            Event::Transaction(transaction) => transaction.da_height(),
        }
    }

    /// Get hashed value of the event.
    pub fn hash(&self) -> Bytes32 {
        match self {
            Event::Message(message) => (*message.message_id().deref()).into(),
            Event::Transaction(transaction) => transaction.id().into(),
        }
    }

    /// Returns the cost of the event to execute.
    pub fn cost(&self) -> u64 {
        match self {
            Event::Message(_message) => 0,
            Event::Transaction(transaction) => transaction.max_gas(),
        }
    }
}

impl From<Message> for Event {
    fn from(message: Message) -> Self {
        Event::Message(message)
    }
}

impl From<RelayedTransaction> for Event {
    fn from(transaction: RelayedTransaction) -> Self {
        Event::Transaction(transaction)
    }
}
