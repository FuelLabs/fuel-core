#[derive(Debug, Clone)]
pub struct Config {
    pub max_block_notify_buffer: usize,
    pub metrics: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_block_notify_buffer: 1 << 10,
            metrics: false,
        }
    }
}
