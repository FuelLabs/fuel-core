use ethers_core::types::Log;
use fuel_types::{Address, AssetId, Bytes32, Word};

use crate::config;

#[derive(Debug, Clone, PartialEq)]
pub enum EthEventLog {
    AssetDeposit {
        account: Address,
        token: AssetId,
        block_number: u32,
        amount: Word,
        deposit_nonce: Bytes32,
    },
    ValidatorDeposit {
        depositor: Address,
        deposit: Word,
    },
    //ValidatorDelagated, Simon mentioned it in chat but there is no events in current contract.
    ValidatorWithdrawal {
        withdrawer: Address,
        withdrawal: Word,
    },
    FuelBlockCommited {
        block_root: Bytes32,
        height: Word,
        da_height: u64,
    },
}

// block_number(32bits) | amount(256bits) | depositNonce(256bits)
const ASSET_DEPOSIT_DATA_LEN: usize = 4 + 32 + 32;

impl TryFrom<&Log> for EthEventLog {
    type Error = &'static str;

    fn try_from(log: &Log) -> Result<Self, Self::Error> {
        if log.topics.is_empty() {
            return Err("Topic list is empty");
        }

        // TODO extract event name-hashes as static with proper values.
        let log = match log.topics[0] {
            n if n == *config::ETH_ASSET_DEPOSIT => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for AssetDeposit");
                }
                let account = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };
                let token = unsafe { AssetId::from_slice_unchecked(log.topics[2].as_ref()) };
                // data is contains: block_number(32bits) | amount(256bits) | depositNonce(256bits)
                let data = &log.data.0;
                if data.len() != ASSET_DEPOSIT_DATA_LEN {
                    return Err("Malformed data length for AssetDeposit");
                }
                if !data[4..28].iter().all(|&b| b == 0) {
                    return Err("Malformed amount for AssetDeposit. Amount bigger then u64");
                }

                let block_number = <[u8; 4]>::try_from(&data[..4])
                    .map(u32::from_be_bytes)
                    .expect("We have checked slice bounds");

                let amount = <[u8; 8]>::try_from(&data[28..36])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                let deposit_nonce = unsafe { Bytes32::from_slice_unchecked(&data[36..]) };

                Self::AssetDeposit {
                    block_number,
                    account,
                    token,
                    amount,
                    deposit_nonce,
                }
            }
            n if n == *config::ETH_VALIDATOR_DEPOSIT => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for ValidatorDeposit");
                }
                // Safety: Casting between same sized structures. It is okay not to check size.
                let depositor = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };

                let deposit = log.topics[2].as_ref();
                if !deposit[..24].iter().all(|&b| b == 0) {
                    return Err("Malformed deposit for ValidatorDeposit. deposit bigger then u64");
                }
                let deposit = <[u8; 8]>::try_from(&deposit[24..])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                Self::ValidatorDeposit { depositor, deposit }
            }
            n if n == *config::ETH_VALIDATOR_WITHDRAWAL => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for ValidatorWithdrawal");
                }
                // Safety: Casting between same sized structures. It is okay not to check size.
                let withdrawer = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };

                let withdrawal = log.topics[2].as_ref();
                if !withdrawal[..24].iter().all(|&b| b == 0) {
                    return Err("Malformed deposit for ValidatorDeposit. deposit bigger then u64");
                }
                let withdrawal = <[u8; 8]>::try_from(&withdrawal[24..])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                Self::ValidatorWithdrawal {
                    withdrawer,
                    withdrawal,
                }
            }
            n if n == *config::ETH_FUEL_BLOCK_COMMITED => {
                if log.topics.len() != 4 {
                    return Err("Malformed topics for FuelBlockCommited");
                }
                // Safety: Casting between same sized structures. It is okay not to check size.
                let block_root = unsafe { Bytes32::from_slice_unchecked(log.topics[1].as_ref()) };

                let height = <[u8; 4]>::try_from(&log.topics[2][28..])
                    .map(u32::from_be_bytes)
                    .expect("Slice bounds are predefined") as u64;
                let da_height = <[u8; 4]>::try_from(&log.topics[3][28..])
                    .map(u32::from_be_bytes)
                    .expect("Slice bounds are predefined") as u64;

                Self::FuelBlockCommited {
                    block_root,
                    height,
                    da_height,
                }
            }

            _ => return Err("Unknown event"),
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

    pub fn eth_log_validator_deposit(eth_block: u64, depositor: Address, deposit: Word) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_VALIDATOR_DEPOSIT,
                H256::from_slice(depositor.as_ref()),
                H256::from_low_u64_be(deposit),
            ],
            Bytes::new(),
        )
    }

    pub fn eth_log_validator_withdrawal(
        eth_block: u64,
        withdrawer: Address,
        withdrawal: Word,
    ) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_VALIDATOR_WITHDRAWAL,
                H256::from_slice(withdrawer.as_ref()),
                H256::from_low_u64_be(withdrawal),
            ],
            Bytes::new(),
        )
    }

    pub fn eth_log_asset_deposit(
        eth_block: u64,
        account: Address,
        token: AssetId,
        block_number: u32,
        amount: Word,
        deposit_nonce: Bytes32,
    ) -> Log {
        //block_number(32bits) | amount(256bits) | depositNonce(256bits)
        // 4+32+32
        let mut b = BytesMut::new();
        b.resize(68, 0);
        //let mut b: [u8; 68] = [0; 68];
        b[..4].copy_from_slice(&block_number.to_be_bytes());
        // 4..28 are zeroes
        b[28..36].copy_from_slice(&amount.to_be_bytes());
        b[36..68].copy_from_slice(deposit_nonce.as_ref());
        log_default(
            eth_block,
            vec![
                *config::ETH_ASSET_DEPOSIT,
                H256::from_slice(account.as_ref()),
                H256::from_slice(token.as_ref()),
            ],
            b.freeze(),
        )
    }

    pub fn eth_log_fuel_block_commited(
        eth_block: u64,
        block_root: Bytes32,
        fuel_height: u32,
        da_height: u32,
    ) -> Log {
        log_default(
            eth_block,
            vec![
                *config::ETH_FUEL_BLOCK_COMMITED,
                H256::from_slice(block_root.as_ref()),
                H256::from_low_u64_be(fuel_height as u64),
                H256::from_low_u64_be(da_height as u64),
            ],
            Bytes::new(),
        )
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
    fn eth_event_validator_withdrawal_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let withdrawer = rng.gen();
        let withdrawal = rng.gen();

        let log = eth_log_validator_withdrawal(eth_block, withdrawer, withdrawal);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::ValidatorWithdrawal {
                withdrawer,
                withdrawal
            },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_validator_deposit_try_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let depositor = rng.gen();
        let deposit = rng.gen();

        let log = eth_log_validator_deposit(eth_block, depositor, deposit);
        assert_eq!(
            Some(U64([eth_block])),
            log.block_number,
            "Block number not set"
        );
        let fuel_log = EthEventLog::try_from(&log);
        assert!(fuel_log.is_ok(), "Parsing error:{:?}", fuel_log);

        assert_eq!(
            fuel_log.unwrap(),
            EthEventLog::ValidatorDeposit { depositor, deposit },
            "Decoded log does not match data we encoded"
        );
    }

    #[test]
    fn eth_event_fuel_block_comit_from_log() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let eth_block = rng.gen();
        let block_root = rng.gen();
        let height: u32 = rng.gen();
        let da_height: u32 = rng.gen();

        let log = eth_log_fuel_block_commited(eth_block, block_root, height, da_height);
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
            EthEventLog::FuelBlockCommited {
                block_root,
                height,
                da_height: da_height as u64,
            },
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

        let log = eth_log_asset_deposit(
            eth_block,
            account,
            token,
            block_number,
            amount,
            deposit_nonce,
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
            EthEventLog::AssetDeposit {
                account,
                token,
                block_number,
                amount,
                deposit_nonce
            },
            "Decoded log does not match data we encoded"
        );
    }
}
