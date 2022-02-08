use std::str::FromStr;

use ethers_core::types::{Log, H256};
use fuel_types::{Address, Bytes32, Color, Word};

pub enum EthEventLog {
    AssetDeposit {
        block_number: Word,
        account: Address,
        token: Color,
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
            n if n == H256::from_str("AssetDeposit").unwrap() => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for AssetDeposit");
                }
                let account = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };
                let token = unsafe { Color::from_slice_unchecked(log.topics[2].as_ref()) };
                // data is contains: block_number(32bits) | amount(256bits) | depositNonce(256bits)
                let data = &log.data.0;
                if data.len() != ASSET_DEPOSIT_DATA_LEN {
                    return Err("Malformed data length for AssetDeposit");
                }
                if !data[4..28].into_iter().all(|&b| b == 0) {
                    return Err("Malformed amount for AssetDeposit. Amount bigger then u64");
                }

                let block_number = <[u8; 4]>::try_from(&data[..4])
                    .map(u32::from_be_bytes)
                    .expect("We have checked slice bounds")
                    as u64;

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
            n if n == H256::from_str("ValidatorDeposit").unwrap() => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for ValidatorDeposit");
                }
                // Safety: Casting between same sized structures. It is okay not to check size.
                let depositor = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };

                let deposit = log.topics[2].as_ref();
                if !deposit[..24].into_iter().all(|&b| b == 0) {
                    return Err("Malformed deposit for ValidatorDeposit. deposit bigger then u64");
                }
                let deposit = <[u8; 8]>::try_from(&deposit[24..])
                    .map(u64::from_be_bytes)
                    .expect("We have checked slice bounds");

                Self::ValidatorDeposit { depositor, deposit }
            }
            n if n == H256::from_str("ValidatorWithdrawal").unwrap() => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for ValidatorWithdrawal");
                }
                // Safety: Casting between same sized structures. It is okay not to check size.
                let withdrawer = unsafe { Address::from_slice_unchecked(log.topics[1].as_ref()) };

                let withdrawal = log.topics[2].as_ref();
                if !withdrawal[..24].into_iter().all(|&b| b == 0) {
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
            n if n == H256::from_str("FuelBlockCommited").unwrap() => {
                if log.topics.len() != 3 {
                    return Err("Malformed topics for FuelBlockCommited");
                }
                // Safety: Casting between same sized structures. It is okay not to check size.
                let block_root = unsafe { Bytes32::from_slice_unchecked(log.topics[1].as_ref()) };

                let height = <[u8; 4]>::try_from(&log.topics[2][28..])
                    .map(u32::from_be_bytes)
                    .expect("Slice bounds are predefined define")
                    as u64;

                Self::FuelBlockCommited { block_root, height }
            }

            _ => return Err("Unknown event"),
        };

        Ok(log)
    }
}
