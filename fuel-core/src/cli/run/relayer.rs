use clap::Args;
use fuel_core_interfaces::model::DaBlockHeight;
use fuel_relayer::{Config, H160};
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Clone, Args)]
pub struct RelayerArgs {
    /// Uri address to ethereum client. It can be in format of `http://localhost:8545/` or `ws://localhost:8545/`.
    /// If not set relayer will not start.
    #[clap(long = "relayer")]
    pub eth_client: Option<String>,

    /// Block number after we can start filtering events related to fuel.
    /// It does not need to be accurate and can be set in past before contracts are deployed.
    #[clap(long = "relayer-v2-deployment", default_value = "0")]
    pub eth_v2_contracts_deployment: DaBlockHeight,

    /// Ethereum contract address. Create EthAddress into fuel_types
    #[clap(long = "relayer-v2-listening-contracts", parse(try_from_str = parse_h160))]
    pub eth_v2_listening_contracts: Vec<H160>,

    /// Number of da block after which messages/stakes/validators become finalized.
    #[clap(long = "relayer-da-finalization", default_value = "64")]
    pub da_finalization: u32,

    /// contract to publish commit fuel block.  
    #[clap(long = "relayer-v2-commit-contract", parse(try_from_str = parse_h160))]
    pub eth_v2_commit_contract: Option<H160>,

    /// ethereum chain_id, default is mainnet
    #[clap(long = "relayer-eth-chain-id", default_value = "1")]
    pub eth_chain_id: u64,

    /// number of blocks that will be asked at one time from client, used for initial sync
    #[clap(long = "relayer-init-sync-step", default_value = "1000")]
    pub initial_sync_step: usize,

    /// Refresh rate of waiting for eth client to finish its initial sync in seconds.
    #[clap(long = "relayer-init-sync-refresh", default_value = "10")]
    pub initial_sync_refresh: u64,
}

pub fn parse_h160(input: &str) -> Result<H160, <H160 as FromStr>::Err> {
    H160::from_str(input)
}

impl From<RelayerArgs> for Config {
    fn from(args: RelayerArgs) -> Self {
        Config {
            da_finalization: args.da_finalization,
            eth_client: args.eth_client,
            eth_chain_id: args.eth_chain_id,
            eth_v2_commit_contract: args.eth_v2_commit_contract,
            eth_v2_listening_contracts: args.eth_v2_listening_contracts,
            eth_v2_contracts_deployment: args.eth_v2_contracts_deployment,
            initial_sync_step: args.initial_sync_step,
            initial_sync_refresh: Duration::from_secs(args.initial_sync_refresh),
        }
    }
}
