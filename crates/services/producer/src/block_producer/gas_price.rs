use fuel_core_types::fuel_types::BlockHeight;

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

/// Interface for retrieving the gas price for a block
pub trait ProducerGasPrice {
    /// The gas price for all transactions in the block.
    fn gas_price(&self, params: GasPriceParams) -> u64;
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
    fn gas_price(&self, _params: GasPriceParams) -> u64 {
        self.gas_price
    }
}
