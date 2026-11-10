use clap::{
    Args,
    builder::ArgPredicate::IsPresent,
};
use fuel_core::{
    relayer::{
        Address,
        Config,
    },
    types::blockchain::primitives::DaBlockHeight,
};
use fuel_core_types::fuel_types::Bytes20;

#[derive(Debug, Clone, Args)]
pub struct RelayerArgs {
    /// Wrapped HTTP routes paired with --relayer. Enables early Ethereum ingestion.
    #[arg(long, value_delimiter = ',', env, requires = "fcr_state")]
    pub relayer_fcr_rpc: Vec<url::Url>,
    /// Safety file on the database PVC; required when using FCR.
    #[arg(long = "relayer-fcr-state", env = "RELAYER_FCR_STATE")]
    pub fcr_state: Option<std::path::PathBuf>,
    /// Expected Ethereum chain ID for FCR (1 or 11155111).
    #[arg(
        long = "relayer-fcr-chain-id",
        env = "RELAYER_FCR_CHAIN_ID",
        default_value_t = 1
    )]
    pub fcr_chain_id: u64,
    /// Recovery generation. A changed generation requires offline recovery of stored state.
    #[arg(
        long = "relayer-fcr-generation",
        env = "RELAYER_FCR_GENERATION",
        default_value = "0"
    )]
    pub fcr_generation: String,
    /// Maximum unchecked early block exposure.
    #[arg(
        long = "relayer-fcr-max-unchecked-blocks",
        env = "RELAYER_FCR_MAX_UNCHECKED_BLOCKS",
        default_value_t = 128
    )]
    pub fcr_max_unchecked_blocks: u64,
    /// Disable early admission while retaining FCR checking and safety state.
    #[arg(
        long = "relayer-fcr-finalized-only",
        env = "RELAYER_FCR_FINALIZED_ONLY"
    )]
    pub fcr_finalized_only: bool,
    /// Maximum seconds of unchecked early exposure before freezing new early admission.
    #[arg(
        long = "relayer-fcr-max-unchecked-seconds",
        env = "RELAYER_FCR_MAX_UNCHECKED_SECONDS",
        default_value_t = 1800
    )]
    pub fcr_max_unchecked_seconds: u64,

    /// Enable the Relayer. By default, the Relayer is disabled, even when the binary is compiled
    /// with the "relayer" feature flag. Providing `--enable-relayer` will enable the relayer
    /// service.
    #[clap(long = "enable-relayer", action)]
    pub enable_relayer: bool,

    /// Uri addresses to ethereum client. It can be in format of `http://localhost:8545/` or `ws://localhost:8545/`.
    /// If not set relayer will not start.
    #[arg(long = "relayer", value_delimiter = ',', env)]
    #[arg(required_if_eq("enable_relayer", "true"))]
    #[arg(requires_if(IsPresent, "enable_relayer"))]
    pub relayer: Option<Vec<url::Url>>,

    /// Ethereum contract address. Create EthAddress into fuel_types
    #[arg(long = "relayer-v2-listening-contracts", value_delimiter = ',', env)]
    pub eth_v2_listening_contracts: Vec<Bytes20>,

    /// Number of da block that the contract is deployed at.
    #[clap(long = "relayer-da-deploy-height", default_value_t = Config::DEFAULT_DA_DEPLOY_HEIGHT, env
    )]
    pub da_deploy_height: u64,

    /// Number of pages or blocks containing logs that
    /// should be downloaded in a single call to the da layer
    #[clap(long = "relayer-log-page-size", default_value_t = Config::DEFAULT_LOG_PAGE_SIZE, env)]
    pub log_page_size: u64,

    /// The maximum number of logs that can be returned in a single RPC call.
    /// Used by the page sizer to shrink the block range if this threshold is exceeded.
    #[clap(long = "relayer-max-logs-per-rpc", default_value_t = Config::DEFAULT_MAX_LOGS_PER_RPC, env
    )]
    pub max_logs_per_rpc: u64,

    /// The minimum duration that the relayer polling loop
    /// will take before running again. If this is too low the DA layer
    /// risks being spammed.
    #[clap(long = "relayer-min-duration", default_value = "5s", env)]
    pub sync_minimum_duration: humantime::Duration,

    #[clap(long = "relayer-eth-sync-call-freq", default_value = "5s", env)]
    pub syncing_call_frequency: humantime::Duration,

    #[clap(long = "relayer-eth-sync-log-freq", default_value = "60s", env)]
    pub syncing_log_frequency: humantime::Duration,
}

impl RelayerArgs {
    pub fn into_config(self) -> Option<Config> {
        if !self.enable_relayer {
            tracing::info!("Relayer service disabled");
            return None;
        }

        let fcr = if self.relayer_fcr_rpc.is_empty() {
            None
        } else {
            Some(fuel_core::relayer::fcr::Config {
                rpc: self.relayer_fcr_rpc,
                state_path: self.fcr_state.expect("clap requires FCR state"),
                chain_id: self.fcr_chain_id,
                generation: self.fcr_generation,
                max_unchecked_blocks: self.fcr_max_unchecked_blocks,
                finalized_only: self.fcr_finalized_only,
                max_unchecked_seconds: self.fcr_max_unchecked_seconds,
            })
        };
        let config = Config {
            fcr,
            da_deploy_height: DaBlockHeight(self.da_deploy_height),
            relayer: self.relayer,
            eth_v2_listening_contracts: self
                .eth_v2_listening_contracts
                .iter()
                .map(|addr| Address::from_slice(addr.as_slice()))
                .collect(),
            log_page_size: self.log_page_size,
            max_logs_per_rpc: self.max_logs_per_rpc,
            sync_minimum_duration: self.sync_minimum_duration.into(),
            syncing_call_frequency: self.syncing_call_frequency.into(),
            syncing_log_frequency: self.syncing_log_frequency.into(),
            metrics: false,
        };
        Some(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use test_case::test_case;

    #[derive(Debug, Clone, Parser)]
    pub struct Command {
        #[clap(flatten)]
        relayer: RelayerArgs,
    }

    #[test_case(&[""] => Ok(None); "no args")]
    #[test_case(&["", "--enable-relayer", "--relayer=https://test.com"] => Ok(Some(vec![url::Url::parse("https://test.com").unwrap()])); "one relayer"
    )]
    #[test_case(&["", "--enable-relayer", "--relayer=https://test.com", "--relayer=https://test2.com"] => Ok(Some(vec![url::Url::parse("https://test.com").unwrap(), url::Url::parse("https://test2.com").unwrap()])); "two relayers in different args"
    )]
    #[test_case(&["", "--enable-relayer", "--relayer=https://test.com,https://test2.com"] => Ok(Some(vec![url::Url::parse("https://test.com").unwrap(), url::Url::parse("https://test2.com").unwrap()])); "two relayers in same arg"
    )]
    fn parse_relayer_urls(args: &[&str]) -> Result<Option<Vec<url::Url>>, String> {
        let command: Command =
            Command::try_parse_from(args).map_err(|e| e.to_string())?;
        let args = command.relayer;
        Ok(args.relayer)
    }
}
