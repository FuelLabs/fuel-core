use ethers_contract::EthEvent;
use ethers_core::types::H256;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::Bytes20,
};
use once_cell::sync::Lazy;
use std::{
    str::FromStr,
    time::Duration,
};

pub(crate) static ETH_LOG_MESSAGE: Lazy<H256> =
    Lazy::new(crate::abi::bridge::MessageSentFilter::signature);

pub(crate) static ETH_FORCED_TX: Lazy<H256> =
    Lazy::new(crate::abi::bridge::TransactionFilter::signature);

// TODO: Move settlement fields into `ChainConfig` because it is part of the consensus.
#[derive(Clone, Debug)]
/// Configuration settings for the Relayer.
pub struct Config {
    /// The da block to which the contract was deployed.
    pub da_deploy_height: DaBlockHeight,
    /// Uri addresses to ethereum client.
    pub relayer: Option<Vec<url::Url>>,
    // TODO: Create `EthAddress` into `fuel_core_types`.
    /// Ethereum contract address.
    pub eth_v2_listening_contracts: Vec<Bytes20>,
    /// Number of pages or blocks containing logs that
    /// should be downloaded in a single call to the da layer
    pub log_page_size: u64,
    /// This throttles the background relayer loop to
    /// at least this duration to prevent spamming the DA node.
    pub sync_minimum_duration: Duration,
    /// How often calls are made to the DA node when the DA node
    /// is in the process of syncing.
    pub syncing_call_frequency: Duration,
    /// How often progress logs are printed when the DA node is
    /// syncing.
    pub syncing_log_frequency: Duration,

    /// Enables metrics on this fuel service
    pub metrics: bool,
}

#[allow(missing_docs)]
impl Config {
    pub const DEFAULT_LOG_PAGE_SIZE: u64 = 10_000;
    pub const DEFAULT_DA_DEPLOY_HEIGHT: u64 = 0;
    pub const DEFAULT_SYNC_MINIMUM_DURATION: Duration = Duration::from_secs(5);
    pub const DEFAULT_SYNCING_CALL_FREQ: Duration = Duration::from_secs(5);
    pub const DEFAULT_SYNCING_LOG_FREQ: Duration = Duration::from_secs(60);
}

impl Default for Config {
    fn default() -> Self {
        Self {
            da_deploy_height: DaBlockHeight::from(Self::DEFAULT_DA_DEPLOY_HEIGHT),
            relayer: None,
            eth_v2_listening_contracts: vec![Bytes20::from_str(
                "0x03E4538018285e1c03CCce2F92C9538c87606911",
            )
            .unwrap()],
            log_page_size: Self::DEFAULT_LOG_PAGE_SIZE,
            sync_minimum_duration: Self::DEFAULT_SYNC_MINIMUM_DURATION,
            syncing_call_frequency: Self::DEFAULT_SYNCING_CALL_FREQ,
            syncing_log_frequency: Self::DEFAULT_SYNCING_LOG_FREQ,
            metrics: false,
        }
    }
}

mod tests {

    #[test]
    fn conversion_str_h160_bytes() {
        use std::str::FromStr;

        let bytes20 = fuel_core_types::fuel_types::Bytes20::from_str(
            "0x03E4538018285e1c03CCce2F92C9538c87606911",
        )
        .unwrap();
        let h160 = ethers_core::types::H160::from_str(
            "0x03E4538018285e1c03CCce2F92C9538c87606911",
        )
        .unwrap();
        assert_eq!(bytes20.as_slice(), h160.as_bytes());
    }
}
