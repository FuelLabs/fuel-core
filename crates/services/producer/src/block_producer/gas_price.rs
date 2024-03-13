use fuel_core_types::fuel_types::BlockHeight;
use thiserror::Error;

/// The parameters required to retrieve the gas price for a block
pub struct GasPriceParams {
    block_height: BlockHeight,
}

impl GasPriceParams {
    /// Create a new `GasPriceParams` instance
    pub fn new(block_height: BlockHeight) -> Self {
        Self { block_height }
    }

    pub fn block_height(&self) -> BlockHeight {
        self.block_height
    }
}

impl From<BlockHeight> for GasPriceParams {
    fn from(block_height: BlockHeight) -> Self {
        Self { block_height }
    }
}

#[derive(Error, Debug)]
pub enum GasPriceError {
    #[error("Failed to retrieve gas price for block: {0}")]
    GasPriceRetrievalFailed(anyhow::Error),
}

/// Interface for retrieving the gas price for a block
pub trait ProducerGasPrice {
    /// The gas price for all transactions in the block.
    fn gas_price(&self, params: GasPriceParams) -> Result<u64, GasPriceError>;
}

pub struct StaticGasPrice {
    pub gas_price: u64,
}

impl StaticGasPrice {
    pub fn new(gas_price: u64) -> Self {
        Self { gas_price }
    }
}

impl ProducerGasPrice for StaticGasPrice {
    fn gas_price(&self, _params: GasPriceParams) -> Result<u64, GasPriceError> {
        Ok(self.gas_price)
    }
}
