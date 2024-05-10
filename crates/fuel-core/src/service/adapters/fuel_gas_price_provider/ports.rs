use fuel_core_types::fuel_types::BlockHeight;

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub enum Error {}

pub struct BlockFullness;

pub trait FuelBlockHistory {
    // type BlockProductionReward;
    fn latest_height(&self) -> BlockHeight;

    fn gas_price(&self, height: BlockHeight) -> Option<u64>;

    fn block_fullness(&self, height: BlockHeight) -> Option<BlockFullness>;

    fn production_reward(&self, height: BlockHeight) -> Option<u64>;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> Option<u64>;
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
