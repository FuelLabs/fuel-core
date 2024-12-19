use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V0Metadata {
    /// The gas price to cover the execution of the next block
    pub new_exec_price: u64,
    // Execution
    /// The lowest the algorithm allows the exec gas price to go
    pub min_exec_gas_price: u64,
    /// The Percentage the execution gas price will change in a single block, either increase or decrease
    /// based on the fullness of the last L2 block
    pub exec_gas_price_change_percent: u64,
    /// The height for which the `new_exec_price` is calculated, which should be the _next_ block
    pub l2_block_height: u32,
    /// The threshold of gas usage above and below which the gas price will increase or decrease
    /// This is a percentage of the total capacity of the L2 block
    pub l2_block_fullness_threshold_percent: u64,
}

pub struct V0AlgorithmConfig {
    pub starting_gas_price: u64,
    pub min_gas_price: u64,
    pub gas_price_change_percent: u64,
    pub gas_price_threshold_percent: u64,
}

impl From<AlgorithmUpdaterV0> for V0Metadata {
    fn from(updater: AlgorithmUpdaterV0) -> Self {
        Self {
            new_exec_price: updater.new_exec_price,
            min_exec_gas_price: updater.min_exec_gas_price,
            exec_gas_price_change_percent: updater.exec_gas_price_change_percent,
            l2_block_height: updater.l2_block_height,
            l2_block_fullness_threshold_percent: updater
                .l2_block_fullness_threshold_percent,
        }
    }
}
