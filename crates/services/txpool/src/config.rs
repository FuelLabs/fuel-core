use fuel_core_chain_config::ChainConfig;
use std::time::Duration;

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
    /// Enables prometheus metrics for this fuel-service
    pub metrics: bool,
    /// Transaction TTL
    pub transaction_ttl: Duration,
}

impl Default for Config {
    fn default() -> Self {
        let min_gas_price = 0;
        let utxo_validation = true;
        let metrics = false;
        // 5 minute TTL
        let transaction_ttl = Duration::from_secs(60 * 5);
        Self::new(
            ChainConfig::default(),
            min_gas_price,
            utxo_validation,
            metrics,
            transaction_ttl,
        )
    }
}

impl Config {
    pub fn new(
        chain_config: ChainConfig,
        min_gas_price: u64,
        utxo_validation: bool,
        metrics: bool,
        transaction_ttl: Duration,
    ) -> Self {
        // # Dev-note: If you add a new field, be sure that this field is propagated correctly
        //  in all places where `new` is used.
        Self {
            max_tx: 4064,
            max_depth: 10,
            min_gas_price,
            utxo_validation,
            chain_config,
            metrics,
            transaction_ttl,
        }
    }
}
