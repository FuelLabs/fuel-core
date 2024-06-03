use fuel_core_types::fuel_types::BlockHeight;
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub type ForeignResult<T> = std::result::Result<T, ForeignError>;

type ForeignError = Box<dyn std::error::Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Requested height is too high. Requested: {requested_height}, latest: {latest_height}")]
    AlgorithmNotUpToDate {
        requested_height: BlockHeight,
        latest_height: BlockHeight,
    },
    #[error("Latest block height past requested height. Requested: {requested_height}, latest: {latest_height}")]
    RequestedOldBlockHeight {
        requested_height: BlockHeight,
        latest_height: BlockHeight,
    },
}

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
