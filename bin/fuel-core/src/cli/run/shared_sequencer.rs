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
    #[clap(
        long = "ss-tendermint-api",
        env = "SHARED_SEQUENCER_TENDERMINT_API",
        default_value = "http://127.0.0.1:26657"
    )]
    tendermint_api: String,
    /// The REST address of the sequencer chain blockchain/rest API
    /// (e.g. "http://127.0.0.1:1317")
    #[clap(
        long = "ss-blockchain-api",
        env = "SHARED_SEQUENCER_BLOCKCHAIN_API",
        default_value = "http://127.0.0.1:1317"
    )]
    blockchain_api: String,
    /// Topic to post blocks to
    /// (e.g. "1111111111111111111111111111111111111111111111111111111111111111")
    #[clap(
        long = "ss-topic",
        env = "SHARED_SEQUENCER_TOPIC",
        default_value = "0000000000000000000000000000000000000000000000000000000000000000"
    )]
    topic: Bytes32,
}

#[cfg(feature = "shared-sequencer")]
impl From<Args> for fuel_core_shared_sequencer::Config {
    fn from(val: Args) -> fuel_core_shared_sequencer::Config {
        fuel_core_shared_sequencer::Config {
            enabled: val.enable,
            block_posting_frequency: val.block_posting_frequency.into(),
            tendermint_rpc_api: val.tendermint_api,
            blockchain_rest_api: val.blockchain_api,
            topic: *val.topic,
        }
    }
}
