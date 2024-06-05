#[derive(Debug, Clone, Copy)]
pub struct BlockFullness {
    used: u64,
    capacity: u64,
}

impl BlockFullness {
    pub fn new(used: u64, capacity: u64) -> Self {
        Self { used, capacity }
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }
}

pub trait GasPriceAlgorithm {
    fn gas_price(&self, block_bytes: u64) -> u64;
}
