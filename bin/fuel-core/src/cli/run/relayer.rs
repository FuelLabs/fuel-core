use clap::{
    Args,
    builder::ArgPredicate::IsPresent,
};
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
    #[arg(long = "relayer", value_delimiter = ',', env)]
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
            return None
        }

        let config = Config {
            da_deploy_height: DaBlockHeight(self.da_deploy_height),
            relayer: self.relayer,
            eth_v2_listening_contracts: self.eth_v2_listening_contracts,
            log_page_size: self.log_page_size,
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
    #[test_case(&["", "--enable-relayer", "--relayer=https://test.com"] => Ok(Some(vec![url::Url::parse("https://test.com").unwrap()])); "one relayer")]
    #[test_case(&["", "--enable-relayer", "--relayer=https://test.com", "--relayer=https://test2.com"] => Ok(Some(vec![url::Url::parse("https://test.com").unwrap(), url::Url::parse("https://test2.com").unwrap()])); "two relayers in different args")]
    #[test_case(&["", "--enable-relayer", "--relayer=https://test.com,https://test2.com"] => Ok(Some(vec![url::Url::parse("https://test.com").unwrap(), url::Url::parse("https://test2.com").unwrap()])); "two relayers in same arg")]
    fn parse_relayer_urls(args: &[&str]) -> Result<Option<Vec<url::Url>>, String> {
        let command: Command =
            Command::try_parse_from(args).map_err(|e| e.to_string())?;
        let args = command.relayer;
        Ok(args.relayer)
    }
}
