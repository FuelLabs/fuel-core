use std::convert::TryFrom;

use bytes::{
    Bytes,
    BytesMut,
};
use ethers_core::types::{
    Bytes as EthersBytes,
    Log,
    H160,
    H256,
    U64,
};
use fuel_core_interfaces::{
    common::{
        fuel_merkle::common::Bytes32,
        prelude::Address,
    },
    model::{
        CheckedMessage,
        Message,
    },
};

use crate::{
    config,
    log::EthEventLog,
};

pub mod middleware;

pub trait LogTestHelper {
    fn to_msg(&self) -> CheckedMessage;
}

impl LogTestHelper for Log {
    fn to_msg(&self) -> CheckedMessage {
        match EthEventLog::try_from(self).unwrap() {
            EthEventLog::Message(m) => Message::from(&m).check(),
            _ => panic!("This log does not form a message"),
        }
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
        BytesMut::from_iter(b.into_iter()).freeze(),
    )
}

pub fn eth_log_fuel_block_committed(
    eth_block: u64,
    block_root: Bytes32,
    fuel_height: u32,
) -> Log {
    log_default(
        Default::default(),
        eth_block,
        vec![
            *config::ETH_FUEL_BLOCK_COMMITTED,
            H256::from_slice(block_root.as_ref()),
            H256::from_low_u64_be(fuel_height as u64),
        ],
        Bytes::new(),
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
