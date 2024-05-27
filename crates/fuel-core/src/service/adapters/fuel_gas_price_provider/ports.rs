use fuel_core_types::fuel_types::BlockHeight;
use thiserror::Error;
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub type ForeignResult<T> = std::result::Result<T, ForeignError>;

type ForeignError = Box<dyn std::error::Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Requested block ({requested}) is too high, latest block is {latest}")]
    RequestedBlockHeightTooHigh {
        requested: BlockHeight,
        latest: BlockHeight,
    },
    #[error("Unable to get latest block height: {0:?}")]
    UnableToGetLatestBlockHeight(ForeignError),
    #[error("Unable to get execution gas price: {0:?}")]
    UnableToGetGasPrices(ForeignError),
    #[error("Unable to store gas prices: {0:?}")]
    UnableToStoreGasPrices(ForeignError),
    #[error("Execution gas price not found for block height: {0:?}")]
    GasPricesNotFoundForBlockHeight(BlockHeight),
    #[error("Unable to get block fullness: {0:?}")]
    UnableToGetBlockFullness(ForeignError),
    #[error("Block fullness not found for block height: {0:?}")]
    BlockFullnessNotFoundForBlockHeight(BlockHeight),
    #[error("Unable to get production reward: {0:?}")]
    UnableToGetProductionReward(ForeignError),
    #[error("Production reward not found for block height: {0:?}")]
    ProductionRewardNotFoundForBlockHeight(BlockHeight),
    #[error("Unable to get recording cost: {0:?}")]
    UnableToGetRecordingCost(ForeignError),
    #[error("Recording cost not found for block height: {0:?}")]
    RecordingCostNotFoundForBlockHeight(BlockHeight),
    #[error("Could not convert block usage to percentage: {0}")]
    CouldNotConvertBlockUsageToPercentage(String),
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

pub trait FuelBlockHistory {
    fn latest_height(&self) -> ForeignResult<BlockHeight>;

    fn block_fullness(&self, height: BlockHeight)
        -> ForeignResult<Option<BlockFullness>>;

    fn production_reward(&self, height: BlockHeight) -> ForeignResult<Option<u64>>;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> ForeignResult<Option<u64>>;
}

#[derive(Debug, Clone, Copy)]
pub struct GasPrices {
    pub execution_gas_price: u64,
    pub da_gas_price: u64,
}

impl GasPrices {
    pub fn new(execution_gas_price: u64, da_gas_price: u64) -> Self {
        Self {
            execution_gas_price,
            da_gas_price,
        }
    }

    pub fn execution_gas_price(&self) -> u64 {
        self.execution_gas_price
    }

    pub fn da_gas_price(&self) -> u64 {
        self.da_gas_price
    }
    pub fn total(&self) -> u64 {
        self.execution_gas_price + self.da_gas_price
    }
}

pub trait GasPriceHistory {
    fn gas_prices(&self, height: BlockHeight) -> ForeignResult<Option<GasPrices>>;
    // TODO: Can we make this mutable? The problem is that the `GasPriceProvider` isn't mut, so
    //   we can't hold mutable references. Might be able to make `GasPriceProvider` `&mut self` but will
    //  take some finangling
    fn store_gas_prices(
        &self,
        height: BlockHeight,
        gas_price: &GasPrices,
    ) -> ForeignResult<()>;
}

pub trait GasPriceAlgorithm {
    fn calculate_gas_prices(
        &self,
        previous_gas_prices: GasPrices,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        block_fullness: BlockFullness,
    ) -> GasPrices;

    fn maximum_next_gas_prices(&self, previous_gas_prices: GasPrices) -> GasPrices;
}
