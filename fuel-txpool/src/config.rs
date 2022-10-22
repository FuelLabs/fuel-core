use fuel_chain_config::ChainConfig;

#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of transactions inside the pool
    pub max_tx: usize,
    /// max depth of connected UTXO excluding contracts
    pub max_depth: usize,
    /// The minimum allowed gas price
    pub min_gas_price: u64,
    /// Flag to disable utxo existence and signature checks
    pub utxo_validation: bool,
    /// chain config
    pub chain_config: ChainConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tx: 4064,
            max_depth: 10,
            min_gas_price: 0,
            utxo_validation: true,
            chain_config: ChainConfig::default(),
        }
    }
}
