use crate::{abi, config};
use anyhow::anyhow;
use ethers_contract::EthEvent;
use ethers_core::{
    abi::RawLog,
    types::{Log, U256},
};
use fuel_core_interfaces::{
    common::fuel_types::{Address, AssetId, Bytes32, Word},
    model::{ConsensusId, DepositCoin, ValidatorId},
};

/// This is going to be superseded with MessageLog: https://github.com/FuelLabs/fuel-core/issues/366
#[derive(Debug, Clone, PartialEq)]
pub struct AssetDepositLog {
    pub account: Address,
    pub token: AssetId,
    pub amount: Word,
    pub precision_factor: u8,
    pub block_number: u32,
    pub deposit_nonce: Bytes32,
}

impl From<&AssetDepositLog> for DepositCoin {
    fn from(asset: &AssetDepositLog) -> Self {
        Self {
            owner: asset.account,
            amount: asset.amount,
            asset_id: asset.token, // TODO should this be hash of token_id and precision factor
            nonce: asset.deposit_nonce,
            deposited_da_height: asset.block_number,
            fuel_block_spend: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EthEventLog {
    AssetDeposit(AssetDepositLog),
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
    FuelBlockCommited {
        block_root: Bytes32,
        height: Word,
    },
    Unknown,
}

/// block_number(32bits) | precisionFactor(8bits) | depositNonce(256bits)
/// data is packet as three 256bit/32bytes values
const ASSET_DEPOSIT_DATA_LEN: usize = 32 + 32 + 32;

impl TryFrom<&Log> for EthEventLog {
    type Error = anyhow::Error;

    fn try_from(log: &Log) -> Result<Self, Self::Error> {
        if log.topics.is_empty() {
            return Err(anyhow!("Topic list is empty"));
        }

        let log = match log.topics[0] {
            n if n == *config::ETH_LOG_ASSET_DEPOSIT => {
                if log.topics.len() != 4 {
                    return Err(anyhow!("Malformed topics for AssetDeposit"));
                }
                let account = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };
                let token = unsafe { AssetId::from_slice_unchecked(log.topics[2].as_ref()) };

                if !log.topics[3][..24].iter().all(|&b| b == 0) {
                    return Err(anyhow!(
                        "Malformed amount for AssetDeposit. Amount bigger then u64",
                    ));
                }
                let amount = <[u8; 8]>::try_from(&log.topics[3][24..])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                // data is contains: block_number(32bits) | precisionFactor(8bits) | depositNonce(256bits)
                let data = &log.data.0;

                if data.len() != ASSET_DEPOSIT_DATA_LEN {
                    return Err(anyhow!(
                        "Malformed data length for AssetDeposit: {}",
                        data.len()
                    ));
                }
                if !data[..28].iter().all(|&b| b == 0) {
                    return Err(anyhow!(
                        "Malformed amount for AssetDeposit. Amount bigger then u64",
                    ));
                }

                let block_number = <[u8; 4]>::try_from(&data[28..32])
                    .map(u32::from_be_bytes)
                    .expect("We have checked slice bounds");

                if !data[32..63].iter().all(|&b| b == 0) {
                    return Err(anyhow!(
                        "Malformed amount for AssetDeposit. Amount bigger then u64",
                    ));
                }

                let precision_factor = data[63];

                let deposit_nonce = unsafe { Bytes32::from_slice_unchecked(&data[64..]) };

                Self::AssetDeposit(AssetDepositLog {
                    block_number,
                    account,
                    amount,
                    token,
                    precision_factor,
                    deposit_nonce,
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
            n if n == *config::ETH_FUEL_BLOCK_COMMITED => {
                if log.topics.len() != 3 {
                    return Err(anyhow!("Malformed topics for FuelBlockCommited"));
                }
                let block_root = unsafe { Bytes32::from_slice_unchecked(log.topics[1].as_ref()) };

                let height = <[u8; 4]>::try_from(&log.topics[2][28..])
                    .map(u32::from_be_bytes)
                    .expect("Slice bounds are predefined") as u64;

                Self::FuelBlockCommited { block_root, height }
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

        data.extend(H256::from_low_u64_be(64).as_ref());
        data.extend(H256::from_low_u64_be(64 + del_data.len() as u64).as_ref());

        data.extend(del_data);

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

    pub fn eth_log_asset_deposit(
        eth_block: u64,
        account: Address,
        token: AssetId,
        block_number: u32,
        amount: Word,
        deposit_nonce: Bytes32,
        precision_factor: u8,
    ) -> Log {
        //block_number(32bits) | precision_factor(256bits) | depositNonce(256bits)
        // 32+32+32
        let mut b = BytesMut::new();
        b.resize(ASSET_DEPOSIT_DATA_LEN, 0);
        //let mut b: [u8; 68] = [0; 68];
        b[28..32].copy_from_slice(&block_number.to_be_bytes());
        // 4..28 are zeroes
        b[63] = precision_factor;
        b[64..96].copy_from_slice(deposit_nonce.as_ref());
        log_default(
            eth_block,
            vec![
                *config::ETH_LOG_ASSET_DEPOSIT,
                H256::from_slice(account.as_ref()),
                H256::from_slice(token.as_ref()),
                H256::from_low_u64_be(amount),
            ],
            b.freeze(),
        )
    }

    pub fn eth_log_fuel_block_commited(
        eth_block: u64,
        block_root: Bytes32,
        fuel_height: u32,
    ) -> Log {
        let t = log_default(
            eth_block,
            vec![
                *config::ETH_FUEL_BLOCK_COMMITED,
                H256::from_slice(block_root.as_ref()),
                H256::from_low_u64_be(fuel_height as u64),
            ],
            Bytes::new(),
        );
        t
    }

    fn log_default(eth_block: u64, topics: Vec<H256>, data: Bytes) -> Log {
        Log {
            address: H160::zero(), // we dont check or use this
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
    fn eth_event_fuel_block_comit_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let block_root = rng.gen();
        let height: u32 = rng.gen();

        let log = eth_log_fuel_block_commited(eth_block, block_root, height);
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
            EthEventLog::FuelBlockCommited { block_root, height },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_asset_deposit_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let account = rng.gen();
        let token = rng.gen();
        let block_number: u32 = rng.gen();
        let amount = rng.gen();
        let deposit_nonce = rng.gen();
        let precision_factor = rng.gen();

        let log = eth_log_asset_deposit(
            eth_block,
            account,
            token,
            block_number,
            amount,
            deposit_nonce,
            precision_factor,
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
            EthEventLog::AssetDeposit(AssetDepositLog {
                account,
                token,
                block_number,
                precision_factor,
                amount,
                deposit_nonce
            }),
            "Decoded log does not match data we encoded"
        );
    }
}
