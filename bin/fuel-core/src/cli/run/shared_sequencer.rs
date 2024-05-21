#[derive(Debug, Clone, clap::Args)]
pub struct Args {
    /// If set to true, new blocks will be posted to the shared sequencer chain
    #[clap(long = "enable-ss", action)]
    enable: bool,
    /// Address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    #[clap(long = "ss-api-address", env = "SHARED_SEQUENCER_API_ADDRESS", default="http://127.0.0.1:26657")]
    api_address: String,
    /// Coin denominator for the sequencer fee payment
    /// (e.g. "utest")
    #[clap(long = "ss-coin-denom", env = "SHARED_SEQUENCER_COIN_DENOM", default="utest")]
    coin_denom: String,
    /// Prefix of bech32 addresses on the sequencer chain
    /// (e.g. "fuelsequencer")
    #[clap(long = "ss-account-prefix", env = "SHARED_SEQUENCER_ACCOUNT_PREFIX", default="fuelsequencer")]
    account_prefix: String,
    /// Chain ID of the sequencer chain
    /// (e.g. "fuelsequencer-1")
    #[clap(long = "ss-chain-id", env = "SHARED_SEQUENCER_CHAIN_ID", default="fuelsequencer-1")]
    chain_id: String,
}
