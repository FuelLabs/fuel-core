use std::time::Duration;

/// Endpoints for the shared sequencer client.
#[derive(Debug, Clone)]
pub struct Endpoints {
    /// The RPC address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:26657")
    pub tendermint_rpc_api: String,
    /// The REST address of the sequencer chain tendermint API
    /// (e.g. "http://127.0.0.1:1317")
    pub blockchain_rest_api: String,
}

/// Configuration for the shared sequencer client
#[derive(Debug, Clone)]
pub struct Config {
    /// Whether the sequencer is enabled.
    pub enabled: bool,
    /// The frequency at which to post blocks to the shared sequencer.
    pub block_posting_frequency: Duration,
    /// Endpoints for the shared sequencer client.
    pub endpoints: Option<Endpoints>,
    /// Topic to post blocks to
    pub topic: [u8; 32],
}

impl Config {
    /// Default configuration for locally running shared sequencer node
    pub fn local_node() -> Self {
        Self {
            enabled: false,
            block_posting_frequency: Duration::from_secs(12),
            endpoints: None,
            topic: [0u8; 32],
        }
    }
}
