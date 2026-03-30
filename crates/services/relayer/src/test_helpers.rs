#![allow(clippy::arithmetic_side_effects)]
#![allow(missing_docs)]

use alloy_rpc_types_eth::Log;
use std::convert::TryFrom;

use crate::log::EthEventLog;
use fuel_core_types::entities::{
    Message,
    RelayedTransaction,
};

pub mod page_sizer;

pub trait LogTestHelper {
    fn to_msg(&self) -> Message;
    fn to_tx(&self) -> RelayedTransaction;
}

impl LogTestHelper for Log {
    fn to_msg(&self) -> Message {
        match EthEventLog::try_from(self).unwrap() {
            EthEventLog::Message(m) => Message::from(&m),
            _ => panic!("This log does not form a message"),
        }
    }

    fn to_tx(&self) -> RelayedTransaction {
        match EthEventLog::try_from(self).unwrap() {
            EthEventLog::Transaction(t) => RelayedTransaction::from(t),
            _ => panic!("This log does not form a relayed transaction"),
        }
    }
}

pub fn convert_to_address(bytes: &[u8]) -> alloy_primitives::Address {
    alloy_primitives::Address::from_slice(bytes)
}
