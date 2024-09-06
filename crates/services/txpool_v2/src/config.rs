pub struct Config {
    pub utxo_validation: bool,
    pub max_block_size: u64,
    pub max_block_gas: u64,
    pub max_tx_chain_gas: u64,
}

#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Self {
            utxo_validation: true,
            max_block_gas: 100000000,
            max_block_size: 1000000000,
            max_tx_chain_gas: 100000000 * 10,
        }
    }
}
