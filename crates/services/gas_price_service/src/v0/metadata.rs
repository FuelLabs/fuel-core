use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V0Metadata {
    /// The gas price to cover the execution of the next block
    pub new_exec_price: u64,
    /// The height for which the `new_exec_price` is calculated, which should be the _next_ block
    pub l2_block_height: u32,
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
            l2_block_height: updater.l2_block_height,
        }
    }
}
