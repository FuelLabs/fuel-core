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
    Lazy::new(crate::abi::bridge::message::SentMessageFilter::signature);

#[derive(Clone, Debug)]
/// Configuration settings for the Relayer.
pub struct Config {
    /// The da block to which the contract was deployed.
    pub da_deploy_height: DaBlockHeight,
    /// Number of da blocks after which messages/stakes/validators become finalized.
    pub da_finalization: DaBlockHeight,
    /// Uri address to ethereum client.
    pub eth_client: Option<url::Url>,
    /// Ethereum contract address. Create EthAddress into fuel_types.
    pub eth_v2_listening_contracts: Vec<H160>,
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
}

#[allow(missing_docs)]
impl Config {
    pub const DEFAULT_LOG_PAGE_SIZE: u64 = 5;
    pub const DEFAULT_DA_FINALIZATION: u64 = 100;
    pub const DEFAULT_DA_DEPLOY_HEIGHT: u64 = 0;
    pub const DEFAULT_SYNC_MINIMUM_DURATION: Duration = Duration::from_secs(5);
    pub const DEFAULT_SYNCING_CALL_FREQ: Duration = Duration::from_secs(5);
    pub const DEFAULT_SYNCING_LOG_FREQ: Duration = Duration::from_secs(60);
}

impl Default for Config {
    fn default() -> Self {
        Self {
            da_deploy_height: DaBlockHeight::from(Self::DEFAULT_DA_DEPLOY_HEIGHT),
            da_finalization: DaBlockHeight::from(Self::DEFAULT_DA_FINALIZATION),
            eth_client: None,
            eth_v2_listening_contracts: vec![H160::from_str(
                "0x03E4538018285e1c03CCce2F92C9538c87606911",
            )
            .unwrap()],
            log_page_size: Self::DEFAULT_LOG_PAGE_SIZE,
            sync_minimum_duration: Self::DEFAULT_SYNC_MINIMUM_DURATION,
            syncing_call_frequency: Self::DEFAULT_SYNCING_CALL_FREQ,
            syncing_log_frequency: Self::DEFAULT_SYNCING_LOG_FREQ,
        }
    }
}
