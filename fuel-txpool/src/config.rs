#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of transactions inside the pool
    pub max_tx: usize,
    /// max depth of connected UTXO excluding contracts
    pub max_depth: usize,
    /// The minimum allowed gas price
    pub min_gas_price: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tx: 4064,
            max_depth: 10,
            min_gas_price: 0,
        }
    }
}
