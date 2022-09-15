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
use fuel_core_interfaces::{
    common::fuel_types::{
        Address,
        Bytes32,
        Word,
    },
    model::{
        DaBlockHeight,
        Message,
    },
};

/// Bridge message send from da to fuel network.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MessageLog {
    pub sender: Address,
    pub recipient: Address,
    pub owner: Address,
    pub nonce: Word,
    pub amount: Word,
    pub data: Vec<u8>,
    pub da_height: DaBlockHeight,
}

impl From<&MessageLog> for Message {
    fn from(message: &MessageLog) -> Self {
        Self {
            sender: message.sender,
            recipient: message.recipient,
            nonce: message.nonce,
            amount: message.amount,
            data: message.data.clone(),
            da_height: message.da_height,
            fuel_block_spend: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EthEventLog {
    // Bridge message from da side
    Message(MessageLog),
    FuelBlockCommitted { block_root: Bytes32, height: Word },
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
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for Message"))
                }

                let raw_log = RawLog {
                    topics: log.topics.clone(),
                    data: log.data.to_vec(),
                };

                let message = abi::bridge::SentMessageFilter::decode_log(&raw_log)?;
                let amount = message.amount;
                let data = message.data.to_vec();
                let nonce = message.nonce;
                let owner = Address::from(message.owner);
                let recipient = Address::from(message.recipient);
                let sender = Address::from(message.sender);

                Self::Message(MessageLog {
                    amount,
                    data,
                    nonce,
                    sender,
                    recipient,
                    owner,
                    // Safety: logs without block numbers are rejected by
                    // FinalizationQueue::append_eth_log before the conversion to EthEventLog happens.
                    // If block_number is none, that means the log is pending.
                    da_height: log.block_number.unwrap().as_u64(),
                })
            }
            n if n == *config::ETH_FUEL_BLOCK_COMMITTED => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for FuelBlockCommitted"))
                }
                let block_root =
                    unsafe { Bytes32::from_slice_unchecked(log.topics[1].as_ref()) };

                let height = <[u8; 4]>::try_from(&log.topics[2][28..])
                    .map(u32::from_be_bytes)
                    .expect("Slice bounds are predefined")
                    as u64;

                Self::FuelBlockCommitted { block_root, height }
            }
            _ => Self::Ignored,
        };

        Ok(log)
    }
}

#[cfg(test)]
pub mod tests {

    use ethers_core::types::U64;
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    use super::*;
    use crate::test_helpers::{
        eth_log_fuel_block_committed,
        eth_log_message,
    };

    #[test]
    fn eth_event_fuel_block_commit_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let block_root = rng.gen();
        let height: u32 = rng.gen();

        let log = eth_log_fuel_block_committed(eth_block, block_root, height);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        let height = height as u64;
        let block_root = block_root.into();
        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::FuelBlockCommitted { block_root, height },
            "Decoded log does not match data we encoded"
        );
    }

    // #[test]
    // fn eth_event_message_try_from_log() {
    //     let rng = &mut StdRng::seed_from_u64(2322u64);
    //     let eth_block: u64 = rng.gen();
    //     let owner: [u8; 32] = rng.gen();
    //     let owner_2: [u8; 20] = (&owner[..20]).try_into().unwrap();
    //     let nonce: u32 = rng.gen();
    //     let amount: u32 = rng.gen();
    //     let data: Vec<u8> = vec![1u8];

    //     let log = eth_log_message(
    //         Default::default(),
    //         eth_block,
    //         owner_2.into(),
    //         nonce,
    //         amount,
    //         data.clone(),
    //     );
    //     assert_eq!(
    //         Some(U64([eth_block])),
    //         log.block_number,
    //         "Block number not set"
    //     );
    //     let fuel_log = EthEventLog::try_from(&log);
    //     assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

    //     assert_eq!(
    //         fuel_log.unwrap(),
    //         EthEventLog::Message(MessageLog {
    //             sender: Default::default(),
    //             recipient: Default::default(),
    //             owner: owner.into(),
    //             nonce: nonce as u64,
    //             amount: amount as u64,
    //             data,
    //             da_height: eth_block
    //         }),
    //         "Decoded log does not match data we encoded"
    //     );
    // }
}
