use ethers_core::types::{
    H160,
    H256,
};
use fuel_core_interfaces::model::DaBlockHeight;
use once_cell::sync::Lazy;
use sha3::{
    Digest,
    Keccak256,
};
use std::{
    str::FromStr,
    time::Duration,
};

pub(crate) const REPORT_INIT_SYNC_PROGRESS_EVERY_N_BLOCKS: DaBlockHeight = 1000;
pub(crate) const NUMBER_OF_TRIES_FOR_INITIAL_SYNC: u64 = 10;

pub fn keccak256(data: &'static str) -> H256 {
    let out = Keccak256::digest(data.as_bytes());
    H256::from_slice(out.as_slice())
}

pub(crate) static ETH_LOG_MESSAGE: Lazy<H256> =
    Lazy::new(|| keccak256("SentMessage(bytes32,bytes32,bytes32,uint64,uint64,bytes)"));
pub(crate) static ETH_FUEL_BLOCK_COMMITTED: Lazy<H256> =
    Lazy::new(|| keccak256("BlockCommitted(bytes32,uint32)"));

#[derive(Clone, Debug)]
pub struct Config {
    /// Number of da block after which messages/stakes/validators become finalized.
    pub da_finalization: DaBlockHeight,
    /// Uri address to ethereum client.
    pub eth_client: Option<String>,
    /// Ethereum chain_id.
    pub eth_chain_id: u64,
    /// Contract to publish commit fuel block.  
    pub eth_v2_commit_contract: Option<H160>,
    /// Ethereum contract address. Create EthAddress into fuel_types.
    pub eth_v2_listening_contracts: Vec<H160>,
    /// Block number after we can start filtering events related to fuel.
    /// It does not need to be accurate and can be set in past before contracts are deployed.
    pub eth_v2_contracts_deployment: DaBlockHeight,
    /// Number of blocks that will be asked at one time from client, used for initial sync.
    pub initial_sync_step: usize,
    /// Refresh rate of waiting for eth client to finish its initial sync.
    pub initial_sync_refresh: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            da_finalization: 64,
            // Some(String::from("http://localhost:8545"))
            eth_client: None,
            eth_chain_id: 1, // ethereum mainnet
            eth_v2_commit_contract: None,
            eth_v2_listening_contracts: vec![H160::from_str(
                "0x03E4538018285e1c03CCce2F92C9538c87606911",
            )
            .unwrap()],
            eth_v2_contracts_deployment: 0,
            initial_sync_step: 1000,
            initial_sync_refresh: Duration::from_secs(5),
        }
    }
}

impl Config {
    pub fn eth_v2_contracts_deployment(&self) -> DaBlockHeight {
        self.eth_v2_contracts_deployment
    }

    pub fn eth_v2_listening_contracts(&self) -> &[H160] {
        &self.eth_v2_listening_contracts
    }

    pub fn da_finalization(&self) -> DaBlockHeight {
        self.da_finalization
    }

    pub fn eth_client(&self) -> Option<&str> {
        self.eth_client.as_deref()
    }

    pub fn initial_sync_step(&self) -> usize {
        self.initial_sync_step
    }

    pub fn initial_sync_refresh(&self) -> Duration {
        self.initial_sync_refresh
    }

    pub fn eth_chain_id(&self) -> u64 {
        self.eth_chain_id
    }

    pub fn eth_v2_commit_contract(&self) -> Option<H160> {
        self.eth_v2_commit_contract
    }
}

#[cfg(test)]
mod tests {
    use crate::config::*;

    #[test]
    pub fn test_function_signatures() {
        assert_eq!(
            *ETH_LOG_MESSAGE,
            H256::from_str(
                "0x6e777c34951035560591fac300515942821cca139ab8a514eb117129048e21b2"
            )
            .unwrap()
        );
        assert_eq!(
            *ETH_FUEL_BLOCK_COMMITTED,
            H256::from_str(
                "0xacd88c3d7181454636347207da731b757b80b2696b26d8e1b378d2ab5ed3e872"
            )
            .unwrap()
        );
    }
}
