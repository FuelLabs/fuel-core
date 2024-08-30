use clap::{
    builder::ArgPredicate::IsPresent,
    Args,
};
use core::time::Duration;
use fuel_core::{
    relayer::Config,
    types::blockchain::primitives::DaBlockHeight,
};
use fuel_core_types::fuel_types::Bytes20;

#[derive(Debug, Clone, Args)]
pub struct RelayerArgs {
    /// Enable the Relayer. By default, the Relayer is disabled, even when the binary is compiled
    /// with the "relayer" feature flag. Providing `--enable-relayer` will enable the relayer
    /// service.
    #[clap(long = "enable-relayer", action)]
    pub enable_relayer: bool,

    /// Uri addresses to ethereum client. It can be in format of `http://localhost:8545/` or `ws://localhost:8545/`.
    /// If not set relayer will not start.
    #[arg(long = "relayer", env)]
    #[arg(required_if_eq("enable_relayer", "true"))]
    #[arg(requires_if(IsPresent, "enable_relayer"))]
    pub relayer: Option<Vec<url::Url>>,

    /// Ethereum contract address. Create EthAddress into fuel_types
    #[arg(long = "relayer-v2-listening-contracts", value_delimiter = ',', env)]
    pub eth_v2_listening_contracts: Vec<Bytes20>,

    /// Number of da block that the contract is deployed at.
    #[clap(long = "relayer-da-deploy-height", default_value_t = Config::DEFAULT_DA_DEPLOY_HEIGHT, env)]
    pub da_deploy_height: u64,

    /// Number of pages or blocks containing logs that
    /// should be downloaded in a single call to the da layer
    #[clap(long = "relayer-log-page-size", default_value_t = Config::DEFAULT_LOG_PAGE_SIZE, env)]
    pub log_page_size: u64,

    /// The minimum number of seconds that the relayer polling loop
    /// will take before running again. If this is too low the DA layer
    /// risks being spammed.
    #[clap(long = "relayer-min-duration-s", default_value_t = Config::DEFAULT_SYNC_MINIMUM_DURATION.as_secs(), env)]
    pub sync_minimum_duration_secs: u64,

    #[clap(long = "relayer-eth-sync-call-freq-s", default_value_t = Config::DEFAULT_SYNCING_CALL_FREQ.as_secs(), env)]
    pub syncing_call_frequency_secs: u64,

    #[clap(long = "relayer-eth-sync-log-freq-s", default_value_t = Config::DEFAULT_SYNCING_LOG_FREQ.as_secs(), env)]
    pub syncing_log_frequency_secs: u64,
}

impl RelayerArgs {
    pub fn into_config(self) -> Option<Config> {
        if !self.enable_relayer {
            tracing::info!("Relayer service disabled");
            return None
        }

        let config = Config {
            da_deploy_height: DaBlockHeight(self.da_deploy_height),
            relayer: self.relayer,
            eth_v2_listening_contracts: self.eth_v2_listening_contracts,
            log_page_size: self.log_page_size,
            sync_minimum_duration: Duration::from_secs(self.sync_minimum_duration_secs),
            syncing_call_frequency: Duration::from_secs(self.syncing_call_frequency_secs),
            syncing_log_frequency: Duration::from_secs(self.syncing_log_frequency_secs),
            metrics: false,
        };
        Some(config)
    }
}
