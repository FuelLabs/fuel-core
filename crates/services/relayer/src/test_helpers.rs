#![allow(clippy::arithmetic_side_effects)]
#![allow(missing_docs)]

use std::convert::TryFrom;

use crate::{
    config,
    log::EthEventLog,
};
use bytes::{
    Bytes,
    BytesMut,
};
use ethers_contract::EthEvent;
use ethers_core::{
    abi::Tokenize,
    types::{
        Bytes as EthersBytes,
        Log,
        H160,
        H256,
        U64,
    },
};
use fuel_core_types::{
    entities::{
        Message,
        RelayedTransaction,
    },
    fuel_types::Address,
};

pub mod middleware;

pub trait LogTestHelper {
    fn to_msg(&self) -> Message;
    fn to_tx(&self) -> RelayedTransaction;
}

pub trait EvtToLog {
    fn into_log(self) -> Log;
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

impl EvtToLog for crate::abi::bridge::MessageSentFilter {
    fn into_log(self) -> Log {
        event_to_log(self, &crate::abi::bridge::MESSAGESENT_ABI)
    }
}

impl EvtToLog for crate::abi::bridge::TransactionFilter {
    fn into_log(self) -> Log {
        event_to_log(self, &crate::abi::bridge::MESSAGESENT_ABI)
    }
}

pub fn event_to_log<E>(event: E, abi: &ethers_core::abi::Abi) -> Log
where
    E: EthEvent,
    E: Tokenize,
{
    let e = abi.event(&<E as EthEvent>::name()).unwrap();
    let mut topics = vec![E::signature()];
    let mut data = vec![];
    for (t, indexed) in event
        .into_tokens()
        .into_iter()
        .zip(e.inputs.iter().map(|ep| ep.indexed))
    {
        if indexed {
            let bytes = ethers_core::abi::encode(&[t]);
            if bytes.len() == 32 {
                let t: [u8; 32] = bytes.try_into().unwrap();
                topics.push(t.into());
            } else {
                todo!("Hash the encoded bytes as Keccak256")
            }
        } else {
            data.push(t);
        }
    }
    Log {
        topics,
        data: ethers_core::abi::encode(&data[..]).into(),
        ..Default::default()
    }
}

pub fn eth_log_message(
    address: H160,
    eth_block: u64,
    owner: Address,
    nonce: u32,
    amount: u32,
    data: Vec<u8>,
) -> Log {
    let mut b: Vec<u8> = Vec::new();
    // owner nonce amount data
    // 32 + 32 + 32 + dyn

    b.extend(owner.as_ref());
    b.extend(H256::from_low_u64_be(nonce as u64).as_ref());
    b.extend(H256::from_low_u64_be(amount as u64).as_ref());
    b.extend(H256::from_low_u64_be(128).as_ref());
    b.extend(H256::from_low_u64_be(data.len() as u64).as_ref());

    // data takes as lest 32 bytes;
    let data_size = ((data.len() / 32) + 1) * 32;
    let start = b.len();
    // resize buffer to be able to extend data.
    b.resize(b.len() + data_size, 0);
    for (i, data) in data.iter().enumerate() {
        b[start + i] = *data;
    }

    log_default(
        address,
        eth_block,
        vec![*config::ETH_LOG_MESSAGE, H256::default(), H256::default()],
        BytesMut::from_iter(b).freeze(),
    )
}

fn log_default(address: H160, eth_block: u64, topics: Vec<H256>, data: Bytes) -> Log {
    Log {
        address,
        topics,
        data: EthersBytes(data),
        block_hash: None,
        block_number: Some(U64([eth_block])),
        transaction_hash: None,
        transaction_index: None,
        log_index: None,
        transaction_log_index: None,
        log_type: None,
        removed: Some(false),
    }
}

pub fn convert_to_address(bytes: &[u8]) -> ethereum_types::Address {
    ethereum_types::Address::from_slice(bytes)
}
