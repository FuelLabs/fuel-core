use fuel_core_types::fuel_types::Bytes32;

#[derive(Debug, Clone, clap::Args)]
pub struct Args {
    /// If set to true, new blocks will be posted to the shared sequencer chain
    #[clap(long = "enable-ss", action)]
    enable: bool,
    /// If set to true, new blocks will be posted to the shared sequencer chain
    #[clap(long = "ss-block-posting-frequency", env, default_value = "12s")]
    block_posting_frequency: humantime::Duration,
    /// The RPC address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    #[clap(long = "ss-tendermint-api", env)]
    tendermint_api: Option<String>,
    /// The REST address of the sequencer chain blockchain/rest API
    /// (e.g. "http://127.0.0.1:1317")
    #[clap(long = "ss-blockchain-api", env)]
    blockchain_api: Option<String>,
    /// Topic to post blocks to
    /// (e.g. "1111111111111111111111111111111111111111111111111111111111111111")
    #[clap(
        long = "ss-topic",
        env,
        default_value = "0000000000000000000000000000000000000000000000000000000000000000"
    )]
    topic: Bytes32,
}

#[cfg(feature = "shared-sequencer")]
impl TryFrom<Args> for fuel_core_shared_sequencer::Config {
    type Error = anyhow::Error;

    fn try_from(val: Args) -> anyhow::Result<fuel_core_shared_sequencer::Config> {
        let endpoints = match (val.tendermint_api, val.blockchain_api) {
            (Some(tendermint_api), Some(blockchain_api)) => {
                Some(fuel_core_shared_sequencer::Endpoints {
                    tendermint_rpc_api: tendermint_api,
                    blockchain_rest_api: blockchain_api,
                })
            }
            (None, None) => None,
            _ => {
                return Err(anyhow::anyhow!(
                    "Both tendermint and blockchain API must be set or unset"
                ))
            }
        };

        if endpoints.is_none() && val.enable {
            return Err(anyhow::anyhow!(
                "Shared sequencer is enabled but no endpoints are set"
            ));
        }

        Ok(fuel_core_shared_sequencer::Config {
            enabled: val.enable,
            block_posting_frequency: val.block_posting_frequency.into(),
            endpoints,
            topic: *val.topic,
        })
    }
}
