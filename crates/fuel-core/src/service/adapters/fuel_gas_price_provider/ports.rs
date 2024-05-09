use fuel_core_types::fuel_types::BlockHeight;

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub enum Error {}

pub struct BlockFullness;

pub struct BlockProductionReward;

pub struct FuelBlockGasPriceInputs {
    gas_price: u64,
    // block_fullness: BlockFullness,
    // block_production_reward: Reward,
}

impl FuelBlockGasPriceInputs {
    pub fn new(
        gas_price: u64,
        // block_fullness: BlockFullness,
        // block_production_reward: BlockProductionReward,
    ) -> Self {
        Self {
            gas_price,
            // block_fullness,
            // block_production_reward,
        }
    }

    /// Get the gas price for the block
    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    // /// Get the block fullness
    // pub fn block_fullness(&self) -> &BlockFullness {
    //     &self.block_fullness
    // }
    //
    // /// Get the block production reward
    // pub fn block_production_reward(&self) -> &Reward {
    //     &self.block_production_reward
    // }
}

pub struct DARecordingCost;

pub trait FuelBlockHistory {
    // type BlockProductionReward;
    fn latest_height(&self) -> BlockHeight;

    fn gas_price(&self, height: BlockHeight) -> Option<u64>;

    fn gas_price_inputs(&self, height: BlockHeight) -> Option<FuelBlockGasPriceInputs>;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> Option<DARecordingCost>;
}

#[derive(Debug, Copy, Clone)]
pub struct ProfitAsOfBlock {
    pub profit: i64,
    pub block_height: BlockHeight,
}

pub trait ProducerProfitIndex {
    fn profit(&self) -> ProfitAsOfBlock;
    fn update_profit(
        &mut self,
        block_height: BlockHeight,
        reward: BlockProductionReward,
        cost: DARecordingCost,
    ) -> Result<()>;
}
