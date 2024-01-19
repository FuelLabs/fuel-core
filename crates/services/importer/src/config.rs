use fuel_core_chain_config::ChainConfig;
use fuel_core_types::fuel_types::ChainId;

#[derive(Debug, Clone)]
pub struct Config {
    pub max_block_notify_buffer: usize,
    pub metrics: bool,
    pub chain_id: ChainId,
}

impl Config {
    pub fn new(chain_config: &ChainConfig) -> Self {
        Self {
            max_block_notify_buffer: 1 << 10,
            metrics: false,
            chain_id: chain_config.consensus_parameters.chain_id,
        }
    }
}

#[cfg(test)]
impl Default for Config {
    fn default() -> Self {
        Self {
            max_block_notify_buffer: 1,
            metrics: false,
            chain_id: ChainId::default(),
        }
    }
}
