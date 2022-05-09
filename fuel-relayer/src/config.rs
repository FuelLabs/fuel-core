use ethers_core::types::{H160, H256};
use std::{str::FromStr, time::Duration};

// BlockCommitted(bytes32,uint32)        0xacd88c3d7181454636347207da731b757b80b2696b26d8e1b378d2ab5ed3e872
// Delegation(address,bytes[],uint256[]) 0xb304243c5b5465a0f6a6b44be45b6906650d542c8e1dd33b0630f72b2f454081
// Deposit(address,uint256)              0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c
// ValidatorRegistration(bytes,bytes)    0xb880ae9a41c67ab61e670929983ea383810f2a09e384b5d1e40a6a8d123e643f
// ValidatorUnregistration(bytes)        0xf47db10f5afa43b31c8a897bb251641a7d2b84011c567c906a5df347c183df14
// Withdrawal(address,uint256)           0x7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65
// DepositMade(uint32,address,address,uint8,uint256,uint256)
//                                       0x34dccbe410bb771d28929a3f1ada2323bfb6ae501200c02dc871b287fb558759
// WithdrawalMade(address,address,address,uint256)
//                                       0x779c18fbb35b88ab773ee6b3d87e1d10eb58021e64e0d7808db646f49403d20b
//

lazy_static::lazy_static! {
    pub static ref ETH_ASSET_DEPOSIT : H256 = H256::from_str("0x34dccbe410bb771d28929a3f1ada2323bfb6ae501200c02dc871b287fb558759").unwrap();
    pub static ref ETH_ASSET_WITHDRAWAL : H256 = H256::from_str("0x779c18fbb35b88ab773ee6b3d87e1d10eb58021e64e0d7808db646f49403d20b").unwrap();
    pub static ref ETH_VALIDATOR_REGISTRATION : H256 = H256::from_str("0xb880ae9a41c67ab61e670929983ea383810f2a09e384b5d1e40a6a8d123e643f").unwrap();
    pub static ref ETH_VALIDATOR_UNREGISTRATION : H256 = H256::from_str("0xf47db10f5afa43b31c8a897bb251641a7d2b84011c567c906a5df347c183df14").unwrap();
    pub static ref ETH_DEPOSIT : H256 = H256::from_str("0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c").unwrap();
    pub static ref ETH_WITHDRAWAL : H256 = H256::from_str("0x7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65").unwrap();
    pub static ref ETH_DELEGATION : H256 = H256::from_str("0xb304243c5b5465a0f6a6b44be45b6906650d542c8e1dd33b0630f72b2f454081").unwrap();
    pub static ref ETH_FUEL_BLOCK_COMMITED : H256 = H256::from_str("0xacd88c3d7181454636347207da731b757b80b2696b26d8e1b378d2ab5ed3e872").unwrap();
}

#[derive(Clone, Debug)]
pub struct Config {
    /// number of blocks between eth blocks where deposits/validator set get finalized.
    /// Finalization is done on eth height measurement.
    pub eth_finality_period: u64,
    /// ws address to ethereum client
    pub eth_client: String,
    /// ethereum chain_id
    pub eth_chain_id: u64,
    pub eth_v2_block_commit_contract: H160,
    /// etheruem contract address. Create EthAddress into fuel_types
    pub eth_v2_contract_addresses: Vec<H160>,
    /// contaract deployed on block. Block number after we can start filtering events related to fuel.
    /// It does not need to be aqurate and can be set in past before contracts are deployed.
    pub eth_v2_contract_deployment: u64,
    /// number of blocks that will be asked in one step, for initial sync
    pub initial_sync_step: usize,
    /// how long do we wait between calling eth client if it finished syncing
    pub eth_initial_sync_refresh: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            eth_finality_period: 64,
            eth_client: String::from("wss://localhost:8545"),
            eth_chain_id: 1, // ethereum mainnet
            eth_v2_block_commit_contract: H160::from_str(
                "0x03E4538018285e1c03CCce2F92C9538c87606911",
            )
            .unwrap(),
            eth_v2_contract_addresses: vec![H160::from_str(
                "0x03E4538018285e1c03CCce2F92C9538c87606911",
            )
            .unwrap()],
            eth_v2_contract_deployment: 14_095_090,
            initial_sync_step: 1000,
            eth_initial_sync_refresh: Duration::from_secs(5),
        }
    }
}

impl Config {
    pub fn eth_v2_contract_deployment(&self) -> u64 {
        self.eth_v2_contract_deployment
    }

    pub fn eth_v2_contract_addresses(&self) -> &[H160] {
        &self.eth_v2_contract_addresses
    }

    pub fn eth_finality_period(&self) -> u64 {
        self.eth_finality_period
    }

    pub fn eth_client(&self) -> &str {
        &self.eth_client
    }

    pub fn initial_sync_step(&self) -> usize {
        self.initial_sync_step
    }

    pub fn eth_initial_sync_refresh(&self) -> Duration {
        self.eth_initial_sync_refresh
    }
}
