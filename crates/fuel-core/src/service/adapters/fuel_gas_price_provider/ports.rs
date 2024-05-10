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
    #[error("Unable to get gas price: {0:?}")]
    UnableToGetGasPrice(ForeignError),
    #[error("Gas price not found for block height: {0:?}")]
    GasPriceNotFoundForBlockHeight(BlockHeight),
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
}

pub struct BlockFullness;

pub trait FuelBlockHistory {
    // type BlockProductionReward;
    fn latest_height(&self) -> ForeignResult<BlockHeight>;

    fn gas_price(&self, height: BlockHeight) -> ForeignResult<Option<u64>>;

    fn block_fullness(&self, height: BlockHeight)
        -> ForeignResult<Option<BlockFullness>>;

    fn production_reward(&self, height: BlockHeight) -> ForeignResult<Option<u64>>;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> ForeignResult<Option<u64>>;
}

pub trait GasPriceAlgorithm {
    fn calculate_gas_price(
        &self,
        previous_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        block_fullness: BlockFullness,
    ) -> u64;

    fn maximum_next_gas_price(&self, previous_gas_price: u64) -> u64;
}
