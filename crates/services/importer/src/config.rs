#[derive(Debug, Clone)]
pub struct Config {
    pub max_block_notify_buffer: usize,
    pub metrics: bool,
}

impl Config {
    pub fn new(metrics: bool) -> Self {
        Self {
            max_block_notify_buffer: 1 << 10,
            metrics,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(false)
    }
}
