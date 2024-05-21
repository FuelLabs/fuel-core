#[derive(Debug, Clone)]
pub struct Config {
    /// If set to true, new blocks will be posted to the shared sequencer chain
    enabled: bool,
    /// Address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    sequencer_api: String,
    /// Coin denominator for the sequencer fee payment
    /// (e.g. "utest")
    coin_denom: String,
    /// Prefix of bech32 addresses on the sequencer chain
    /// (e.g. "fuelsequencer")
    account_prefix: String,
    /// Chain ID of the sequencer chain
    /// (e.g. "fuelsequencer-1")
    chain_id: String,
}
