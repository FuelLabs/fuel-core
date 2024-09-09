use std::time::Duration;

pub struct Config {
    pub utxo_validation: bool,
    pub max_block_size: u64,
    pub max_block_gas: u64,
    pub max_txs_per_chain: u64,
    pub max_txs_ttl: Duration,
}

#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Self {
            utxo_validation: true,
            max_block_gas: 100000000,
            max_block_size: 1000000000,
            max_txs_per_chain: 1000,
            max_txs_ttl: Duration::from_secs(60 * 10),
        }
    }
}
