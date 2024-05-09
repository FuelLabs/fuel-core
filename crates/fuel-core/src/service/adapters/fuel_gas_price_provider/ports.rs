use fuel_core_types::fuel_types::BlockHeight;

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
    // fn block_fullness(&self, height: BlockHeight) -> Option<BlockFullness>;
    //
    // fn block_production_reward(
    //     &self,
    //     height: BlockHeight,
    // ) -> Option<Self::BlockProductionReward>;

    fn gas_price_inputs(&self, height: BlockHeight) -> Option<FuelBlockGasPriceInputs>;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> Option<DARecordingCost>;
}
