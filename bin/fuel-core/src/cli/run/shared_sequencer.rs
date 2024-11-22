use fuel_core_types::fuel_types::Bytes32;

#[derive(Debug, Clone, clap::Args)]
pub struct Args {
    /// If set to true, new blocks will be posted to the shared sequencer chain
    #[clap(long = "enable-ss", action)]
    enable: bool,
    /// Address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    #[clap(
        long = "ss-tendermint-api",
        env = "SHARED_SEQUENCER_TENDERMINT_API",
        default_value = "http://127.0.0.1:26657"
    )]
    tendermint_api: String,
    /// Address of the sequencer chain blockchain/rest API
    /// (e.g. "http://127.0.0.1:1317")
    #[clap(
        long = "ss-blockchain-api",
        env = "SHARED_SEQUENCER_BLOCKCHAIN_API",
        default_value = "http://127.0.0.1:1317"
    )]
    blockchain_api: String,
    /// Coin denominator for the sequencer fee payment
    /// (e.g. "utest")
    #[clap(
        long = "ss-coin-denom",
        env = "SHARED_SEQUENCER_COIN_DENOM",
        default_value = "utest"
    )]
    coin_denom: String,
    /// Prefix of bech32 addresses on the sequencer chain
    /// (e.g. "fuelsequencer")
    #[clap(
        long = "ss-account-prefix",
        env = "SHARED_SEQUENCER_ACCOUNT_PREFIX",
        default_value = "fuelsequencer"
    )]
    account_prefix: String,
    /// Chain ID of the sequencer chain
    /// (e.g. "fuelsequencer-1")
    #[clap(
        long = "ss-chain-id",
        env = "SHARED_SEQUENCER_CHAIN_ID",
        default_value = "fuelsequencer-1"
    )]
    chain_id: String,
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
            tendermint_api: val.tendermint_api,
            blockchain_api: val.blockchain_api,
            coin_denom: val.coin_denom,
            account_prefix: val.account_prefix,
            chain_id: val.chain_id,
            topic: *val.topic,
        }
    }
}
