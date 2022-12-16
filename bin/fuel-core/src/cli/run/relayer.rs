use clap::Args;
use core::time::Duration;
use fuel_core::{
    relayer::{
        Config,
        H160,
    },
    types::blockchain::primitives::DaBlockHeight,
};
use std::str::FromStr;

#[derive(Debug, Clone, Args)]
pub struct RelayerArgs {
    /// Uri address to ethereum client. It can be in format of `http://localhost:8545/` or `ws://localhost:8545/`.
    /// If not set relayer will not start.
    #[clap(long = "relayer")]
    pub eth_client: Option<url::Url>,

    /// Ethereum contract address. Create EthAddress into fuel_types
    #[clap(long = "relayer-v2-listening-contracts", parse(try_from_str = parse_h160))]
    pub eth_v2_listening_contracts: Vec<H160>,

    /// Number of da block after which messages/stakes/validators become finalized.
    #[clap(long = "relayer-da-finalization", default_value_t = Config::DEFAULT_DA_FINALIZATION)]
    pub da_finalization: u64,

    /// Number of da block that the contract is deployed at.
    #[clap(long = "relayer-da-deploy-height", default_value_t = Config::DEFAULT_DA_DEPLOY_HEIGHT)]
    pub da_deploy_height: u64,

    /// Number of pages or blocks containing logs that
    /// should be downloaded in a single call to the da layer
    #[clap(long = "relayer-log-page-size", default_value_t = Config::DEFAULT_LOG_PAGE_SIZE)]
    pub log_page_size: u64,

    /// The minimum number of seconds that the relayer polling loop
    /// will take before running again. If this is too low the DA layer
    /// risks being spammed.
    #[clap(long = "relayer-min-duration-s", default_value_t = Config::DEFAULT_SYNC_MINIMUM_DURATION.as_secs())]
    pub sync_minimum_duration_secs: u64,

    #[clap(long = "relayer-eth-sync-call-freq-s", default_value_t = Config::DEFAULT_SYNCING_CALL_FREQ.as_secs())]
    pub syncing_call_frequency_secs: u64,

    #[clap(long = "relayer-eth-sync-log-freq-s", default_value_t = Config::DEFAULT_SYNCING_LOG_FREQ.as_secs())]
    pub syncing_log_frequency_secs: u64,
}

pub fn parse_h160(input: &str) -> Result<H160, <H160 as FromStr>::Err> {
    H160::from_str(input)
}

impl From<RelayerArgs> for Config {
    fn from(args: RelayerArgs) -> Self {
        Config {
            da_deploy_height: DaBlockHeight(args.da_deploy_height),
            da_finalization: DaBlockHeight(args.da_finalization),
            eth_client: args.eth_client,
            eth_v2_listening_contracts: args.eth_v2_listening_contracts,
            log_page_size: args.log_page_size,
            sync_minimum_duration: Duration::from_secs(args.sync_minimum_duration_secs),
            syncing_call_frequency: Duration::from_secs(args.syncing_call_frequency_secs),
            syncing_log_frequency: Duration::from_secs(args.syncing_log_frequency_secs),
            metrics: false,
        }
    }
}
