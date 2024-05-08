use fuel_core_txpool::types::GasPrice;
use fuel_core_types::fuel_types::BlockHeight;

pub struct BlockFullness;

pub struct BlockProductionReward;

pub struct DARecordingCost;

impl PartialEq<DARecordingCost> for BlockProductionReward {
    fn eq(&self, _other: &DARecordingCost) -> bool {
        todo!()
    }
}

pub trait BlockHistory {
    fn latest_height(&self) -> BlockHeight;
}

pub trait GasPriceHistory {
    fn gas_price(&self, height: BlockHeight) -> Option<GasPrice>;
}

pub trait BlockFullnessHistory {
    fn block_fullness(&self, height: BlockHeight) -> BlockFullness;
}

pub trait FuelBlockProductionRewardHistory {
    fn block_production_reward(&self, height: BlockHeight) -> BlockProductionReward;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> DARecordingCost;
}
