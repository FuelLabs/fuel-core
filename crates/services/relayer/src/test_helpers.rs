#![allow(clippy::arithmetic_side_effects)]
#![allow(missing_docs)]

use std::convert::TryFrom;

use crate::{
    // config,
    log::EthEventLog,
};
// use alloy_primitives::{
//     Address,
//     B256,
//     LogData,
// };
// use bytes::{
//     Bytes,
//     BytesMut,
// };
use fuel_core_types::entities::{
    Message,
    RelayedTransaction,
};

// pub mod middleware;

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

// pub fn eth_log_message(
//     address: Address,
//     eth_block: u64,
//     owner: Address,
//     nonce: u32,
//     amount: u32,
//     data: Vec<u8>,
// ) -> Log {
//     let mut b: Vec<u8> = Vec::new();
//     // owner nonce amount data
//     // 32 + 32 + 32 + dyn

//     b.extend(owner.as_ref());
//     b.extend(B256::from_low_u64_be(nonce as u64).as_ref());
//     b.extend(B256::from_low_u64_be(amount as u64).as_ref());
//     b.extend(B256::from_low_u64_be(128).as_ref());
//     b.extend(B256::from_low_u64_be(data.len() as u64).as_ref());

//     // data takes as lest 32 bytes;
//     let data_size = ((data.len() / 32) + 1) * 32;
//     let start = b.len();
//     // resize buffer to be able to extend data.
//     b.resize(b.len() + data_size, 0);
//     for (i, data) in data.iter().enumerate() {
//         b[start + i] = *data;
//     }

//     log_default(
//         address,
//         eth_block,
//         vec![*config::ETH_LOG_MESSAGE, B256::default(), B256::default()],
//         BytesMut::from_iter(b).freeze(),
//     )
// }

use alloy_rpc_types_eth::Log;
// fn log_default(address: Address, eth_block: u64, topics: Vec<B256>, data: Bytes) -> Log {
//     Log {
//         inner: alloy_primitives::Log {
//             address,
//             data: LogData::new_unchecked(topics, data.into()),
//         },
//         block_hash: None,
//         block_number: Some(eth_block),
//         transaction_hash: None,
//         transaction_index: None,
//         log_index: None,
//         removed: false,
//         block_timestamp: None,
//     }
// }

pub fn convert_to_address(bytes: &[u8]) -> ethereum_types::Address {
    ethereum_types::Address::from_slice(bytes)
}
