use ethers_contract::EthEvent;
use ethers_core::types::{
    H160,
    H256,
};
use fuel_core_interfaces::model::DaBlockHeight;
use once_cell::sync::Lazy;
use std::{
    str::FromStr,
    time::Duration,
};

pub(crate) static ETH_LOG_MESSAGE: Lazy<H256> =
    Lazy::new(|| crate::abi::bridge::message::SentMessageFilter::signature());
pub(crate) static ETH_FUEL_BLOCK_COMMITTED: Lazy<H256> =
    Lazy::new(|| crate::fuel::fuel::BlockCommittedFilter::signature());

#[derive(Clone, Debug)]
/// Configuration settings for the [`Relayer`](crate::relayer::Relayer).
pub struct Config {
    /// Number of da block after which messages/stakes/validators become finalized.
    pub da_finalization: DaBlockHeight,
    /// Uri address to ethereum client.
    pub eth_client: Option<url::Url>,
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
    /// Pending eth transaction interval time
    pub pending_eth_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            da_finalization: 100,
            // Some(String::from("http://localhost:8545"))
            eth_client: None,
            eth_chain_id: 1, // ethereum mainnet
            eth_v2_commit_contract: Some(
                H160::from_str("0x03E4538018285e1c03CCce2F92C9538c87606911").unwrap(),
            ),
            eth_v2_listening_contracts: vec![H160::from_str(
                "0x03E4538018285e1c03CCce2F92C9538c87606911",
            )
            .unwrap()],
            eth_v2_contracts_deployment: 0,
            initial_sync_step: 1000,
            initial_sync_refresh: Duration::from_secs(5),
            pending_eth_interval: Duration::from_secs(6),
        }
    }
}

#[allow(missing_docs)]
impl Config {
    pub fn default_test() -> Self {
        let mut s = Self::default();
        s.pending_eth_interval = Duration::from_millis(100);
        s
    }
    pub fn eth_v2_contracts_deployment(&self) -> DaBlockHeight {
        self.eth_v2_contracts_deployment
    }

    pub fn eth_v2_listening_contracts(&self) -> &[H160] {
        &self.eth_v2_listening_contracts
    }

    pub fn da_finalization(&self) -> DaBlockHeight {
        self.da_finalization
    }

    pub fn eth_client(&self) -> Option<&url::Url> {
        self.eth_client.as_ref()
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
