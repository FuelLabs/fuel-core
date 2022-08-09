use crate::{abi, config};
use anyhow::anyhow;
use ethers_contract::EthEvent;
use ethers_core::{
    abi::RawLog,
    types::{Log, U256},
};
use fuel_core_interfaces::model::DaBlockHeight;
use fuel_core_interfaces::{
    common::fuel_types::{Address, Bytes32, Word},
    model::{ConsensusId, Message, ValidatorId},
};

/// Bridge message send from da to fuel network.
#[derive(Debug, Clone, PartialEq)]
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
            owner: message.owner,
            nonce: message.nonce,
            amount: message.amount,
            data: message.data.clone(),
            da_height: message.da_height,
            fuel_block_spend: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EthEventLog {
    // Bridge message from da side
    Message(MessageLog),
    // save it in validator set
    ValidatorRegistration {
        staking_key: ValidatorId,
        consensus_key: ConsensusId,
    },
    // remove it from validator set
    ValidatorUnregistration {
        staking_key: ValidatorId,
    },
    // do nothing. maybe used it for stats or info data.
    Deposit {
        depositor: Address, // It is 24bytes address from ethereum
        amount: Word,
    },
    // remove all delegations
    Withdrawal {
        withdrawer: Address, // It is 24bytes address from ethereum
        amount: Word,
    },
    // remove old delegations, delegate to new validators.
    Delegation {
        delegator: Address, // It is 24bytes address from ethereum
        delegates: Vec<ValidatorId>,
        amounts: Vec<u64>,
    },
    FuelBlockCommitted {
        block_root: Bytes32,
        height: Word,
    },
    Unknown,
}

impl TryFrom<&Log> for EthEventLog {
    type Error = anyhow::Error;

    fn try_from(log: &Log) -> Result<Self, Self::Error> {
        if log.topics.is_empty() {
            return Err(anyhow!("Topic list is empty"));
        }

        let log = match log.topics[0] {
            n if n == *config::ETH_LOG_MESSAGE => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for Message"));
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
            n if n == *config::ETH_LOG_VALIDATOR_REGISTRATION => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for ValidatorRegistration"));
                }
                let staking_key =
                    unsafe { ValidatorId::from_slice_unchecked(log.topics[1].as_ref()) };
                let consensus_key =
                    unsafe { ConsensusId::from_slice_unchecked(log.topics[2].as_ref()) };

                Self::ValidatorRegistration {
                    staking_key,
                    consensus_key,
                }
            }
            n if n == *config::ETH_LOG_VALIDATOR_UNREGISTRATION => {
                if log.topics.len() != 2 {
                    return Err(anyhow!("Malformed topics for ValidatorUnregistration"));
                }
                let staking_key =
                    unsafe { ValidatorId::from_slice_unchecked(log.topics[1].as_ref()) };

                Self::ValidatorUnregistration { staking_key }
            }
            n if n == *config::ETH_LOG_DEPOSIT => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for ValidatorRegistration"));
                }
                let depositor = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };
                let amount = unsafe { Bytes32::from_slice_unchecked(log.topics[2].as_ref()) };

                let amount = <[u8; 8]>::try_from(&amount[24..])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                Self::Deposit { depositor, amount }
            }
            n if n == *config::ETH_LOG_WITHDRAWAL => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for Withdrawal"));
                }
                let withdrawer = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };
                let amount = unsafe { Bytes32::from_slice_unchecked(log.topics[2].as_ref()) };

                let amount = <[u8; 8]>::try_from(&amount[24..])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                Self::Withdrawal { withdrawer, amount }
            }
            n if n == *config::ETH_LOG_DELEGATION => {
                if log.topics.len() != 2 {
                    return Err(anyhow!("Malformed topics for Delegation"));
                }

                let raw_log = RawLog {
                    topics: log.topics.clone(),
                    data: log.data.to_vec(),
                };

                let delegation = abi::validators::DelegationFilter::decode_log(&raw_log)?;
                let mut delegator = Address::zeroed();
                delegator[12..].copy_from_slice(delegation.delegator.as_bytes());

                Self::Delegation {
                    delegator,
                    delegates: delegation
                        .delegates
                        .into_iter()
                        .map(|b| {
                            let mut delegates = ValidatorId::zeroed();
                            delegates[12..].copy_from_slice(b.as_ref());
                            delegates
                        })
                        .collect(),
                    amounts: delegation
                        .amounts
                        .into_iter()
                        .map(|amount| {
                            if amount > U256::from(u64::MAX) {
                                u64::MAX
                            } else {
                                amount.as_u64()
                            }
                        })
                        .collect(),
                }
            }
            n if n == *config::ETH_FUEL_BLOCK_COMMITTED => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for FuelBlockCommitted"));
                }
                let block_root = unsafe { Bytes32::from_slice_unchecked(log.topics[1].as_ref()) };

                let height = <[u8; 4]>::try_from(&log.topics[2][28..])
                    .map(u32::from_be_bytes)
                    .expect("Slice bounds are predefined") as u64;

                Self::FuelBlockCommitted { block_root, height }
            }
            _ => Self::Unknown,
        };

        Ok(log)
    }
}

#[cfg(test)]
pub mod tests {

    use bytes::{Bytes, BytesMut};
    use ethers_core::types::{Bytes as EthersBytes, H160, H256, U64};
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    use super::*;
    use crate::config;

    pub fn eth_log_validator_registration(
        eth_block: u64,
        staking_key: ValidatorId,
        consensus_key: ConsensusId,
    ) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_LOG_VALIDATOR_REGISTRATION,
                H256::from_slice(staking_key.as_ref()),
                H256::from_slice(consensus_key.as_ref()),
            ],
            Bytes::new(),
        )
    }

    pub fn eth_log_validator_unregistration(eth_block: u64, staking_key: ValidatorId) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_LOG_VALIDATOR_UNREGISTRATION,
                H256::from_slice(staking_key.as_ref()),
            ],
            Bytes::new(),
        )
    }

    pub fn eth_log_deposit(eth_block: u64, depositor: Address, amount: Word) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_LOG_DEPOSIT,
                H256::from_slice(depositor.as_ref()),
                H256::from_low_u64_be(amount),
            ],
            Bytes::new(),
        )
    }

    pub fn eth_log_withdrawal(eth_block: u64, withdrawer: Address, amount: Word) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_LOG_WITHDRAWAL,
                H256::from_slice(withdrawer.as_ref()),
                H256::from_low_u64_be(amount),
            ],
            Bytes::new(),
        )
    }

    pub fn eth_log_delegation(
        eth_block: u64,
        mut delegator: Address,
        mut delegates: Vec<ValidatorId>,
        amounts: Vec<u64>,
    ) -> Log {
        delegator.iter_mut().take(12).for_each(|i| *i = 0);

        delegates
            .iter_mut()
            .for_each(|delegate| delegate.iter_mut().take(12).for_each(|i| *i = 0));

        let mut data: Vec<u8> = Vec::new();
        let mut del_data: Vec<u8> = Vec::new();

        del_data.extend(H256::from_low_u64_be(delegates.len() as u64).as_ref());
        // append offsets
        let offset = delegates.len() * 32;
        for i in 0..delegates.len() {
            del_data.extend(H256::from_low_u64_be((offset + i * 64) as u64).as_ref());
        }
        // 20 is size of eth address
        let size = H256::from_low_u64_be(20);
        for delegate in delegates {
            del_data.extend(size.as_ref());
            let mut bytes = [0u8; 32];
            bytes[..20].copy_from_slice(&delegate.as_ref()[12..]);
            del_data.extend(&bytes);
        }

        // index for first item
        data.extend(H256::from_low_u64_be(64).as_ref());
        // index of end of del data
        data.extend(H256::from_low_u64_be(64 + del_data.len() as u64).as_ref());

        // del data
        data.extend(del_data);

        // index of end of amount data
        data.extend(H256::from_low_u64_be(amounts.len() as u64).as_ref());
        for amount in amounts {
            data.extend(H256::from_low_u64_be(amount).as_ref());
        }

        let data = BytesMut::from_iter(data.into_iter()).freeze();

        log_default(
            eth_block,
            vec![
                *config::ETH_LOG_DELEGATION,
                H256::from_slice(delegator.as_ref()),
            ],
            data,
        )
    }

    pub fn eth_log_message(
        eth_block: u64,
        sender: Address,
        receipient: Address,
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
            eth_block,
            vec![
                *config::ETH_LOG_MESSAGE,
                H256::from_slice(sender.as_ref()),
                H256::from_slice(receipient.as_ref()),
            ],
            BytesMut::from_iter(b.into_iter()).freeze(),
        )
    }

    pub fn eth_log_fuel_block_committed(
        eth_block: u64,
        block_root: Bytes32,
        fuel_height: u32,
    ) -> Log {
        let t = log_default(
            eth_block,
            vec![
                *config::ETH_FUEL_BLOCK_COMMITTED,
                H256::from_slice(block_root.as_ref()),
                H256::from_low_u64_be(fuel_height as u64),
            ],
            Bytes::new(),
        );
        t
    }

    fn log_default(eth_block: u64, topics: Vec<H256>, data: Bytes) -> Log {
        Log {
            address: H160::zero(), // we don't check or use this
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

    #[test]
    fn eth_event_validator_unregistration_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let staking_key = rng.gen();

        let log = eth_log_validator_unregistration(eth_block, staking_key);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::ValidatorUnregistration { staking_key },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_validator_registration_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let staking_key = rng.gen();
        let consensus_key = rng.gen();

        let log = eth_log_validator_registration(eth_block, staking_key, consensus_key);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::ValidatorRegistration {
                staking_key,
                consensus_key
            },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_withdrawal_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let withdrawer = rng.gen();
        let amount = rng.gen();

        let log = eth_log_withdrawal(eth_block, withdrawer, amount);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::Withdrawal { withdrawer, amount },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_deposit_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let depositor = rng.gen();
        let amount = rng.gen();

        let log = eth_log_deposit(eth_block, depositor, amount);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::Deposit { depositor, amount },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_delegate_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let mut delegator: Address = rng.gen();
        delegator.iter_mut().take(12).for_each(|i| *i = 0);
        let mut delegate1: ValidatorId = rng.gen();
        delegate1.iter_mut().take(12).for_each(|i| *i = 0);
        let mut delegate2: ValidatorId = rng.gen();
        delegate2.iter_mut().take(12).for_each(|i| *i = 0);

        let log = eth_log_delegation(
            eth_block,
            delegator,
            vec![delegate1, delegate2],
            vec![10, 20],
        );

        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );

        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::Delegation {
                delegator,
                delegates: vec![delegate1, delegate2],
                amounts: vec![10, 20],
            },
            "Decoded log does not match data we encoded"
        );
    }

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
        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::FuelBlockCommitted { block_root, height },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_message_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block: u64 = rng.gen();
        let sender: Address = rng.gen();
        let receipient: Address = rng.gen();
        let owner: Address = rng.gen();
        let nonce: u32 = rng.gen();
        let amount: u32 = rng.gen();
        let data: Vec<u8> = vec![1u8];

        let log = eth_log_message(
            eth_block,
            sender,
            receipient,
            owner,
            nonce,
            amount,
            data.clone(),
        );
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::Message(MessageLog {
                sender,
                recipient: receipient,
                owner,
                nonce: nonce as u64,
                amount: amount as u64,
                data,
                da_height: eth_block
            }),
            "Decoded log does not match data we encoded"
        );
    }
}
