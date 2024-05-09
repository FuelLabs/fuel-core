use fuel_core_types::fuel_types::BlockHeight;

pub struct BlockFullness;

pub struct BlockProductionReward;

pub struct DARecordingCost;

impl PartialEq<DARecordingCost> for BlockProductionReward {
    fn eq(&self, _other: &DARecordingCost) -> bool {
        todo!()
    }
}

pub trait FuelBlockHistory {
    fn latest_height(&self) -> BlockHeight;

    fn gas_price(&self, height: BlockHeight) -> Option<u64>;

    fn block_fullness(&self, height: BlockHeight) -> BlockFullness;

    fn block_production_reward(&self, height: BlockHeight) -> BlockProductionReward;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> DARecordingCost;
}
