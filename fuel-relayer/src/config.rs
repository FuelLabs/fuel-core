use std::{str::FromStr, time::Duration};

use ethers_core::types::H160;

#[derive(Clone, Debug)]
pub struct Config {
    /// number of blocks between eth blocks where deposits/validator set get finalized.
    /// Finalization is done on eth height measurement.
    pub eth_finality_slider: u64,
    /// ws address to ethereum client
    pub eth_client: String,
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

impl Config {
    pub fn new() -> Self {
        Self {
            eth_finality_slider: 64,
            eth_client: String::from(
                "wss://mainnet.infura.io/ws/v3/0954246eab5544e89ac236b668980810",
            ),
            eth_v2_contract_addresses: vec![H160::from_str(
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            )
            .unwrap()],
            eth_v2_contract_deployment: 14_095_090,
            initial_sync_step: 1000,
            eth_initial_sync_refresh: Duration::from_secs(5),
        }
    }

    pub fn eth_v2_contract_deployment(&self) -> u64 {
        self.eth_v2_contract_deployment
    }

    pub fn eth_v2_contract_addresses(&self) -> &[H160] {
        &self.eth_v2_contract_addresses
    }

    pub fn eth_finality_slider(&self) -> u64 {
        self.eth_finality_slider
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

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
