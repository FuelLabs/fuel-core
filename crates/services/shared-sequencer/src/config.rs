use std::time::Duration;

/// Configuration for the shared sequencer client
#[derive(Debug, Clone)]
pub struct Config {
    /// Whether the sequencer is enabled.
    pub enabled: bool,
    /// The frequency at which to post blocks to the shared sequencer.
    pub block_posting_frequency: Duration,
    /// The RPC address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    pub tendermint_rpc_api: String,
    /// The REST address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:1317")
    pub blockchain_rest_api: String,
    /// Topic to post blocks to
    pub topic: [u8; 32],
}

impl Config {
    /// Default configuration for locally running shared sequencer node
    pub fn local_node() -> Self {
        Self {
            enabled: false,
            block_posting_frequency: Duration::from_secs(12),
            tendermint_rpc_api: "http://127.0.0.1:26657".to_owned(),
            blockchain_rest_api: "http://127.0.0.1:1317".to_owned(),
            topic: [0u8; 32],
        }
    }
}
