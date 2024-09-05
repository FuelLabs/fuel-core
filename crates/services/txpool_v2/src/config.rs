pub struct Config {
    pub max_block_size: u64,
    pub max_block_gas: u64,
    pub max_depth: usize,
    pub max_width: usize
}

#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Self {
            max_block_gas: 100000000,
            max_block_size: 1000000000,
            max_depth: 10,
            max_width: 100
        }
    }
}