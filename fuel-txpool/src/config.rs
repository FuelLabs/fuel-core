#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of transaction inside pool
    pub max_tx: usize,
    /// max depth of connected UTXO excluding contracts
    pub max_depth: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tx: 4064,
            max_depth: 10,
        }
    }
}
