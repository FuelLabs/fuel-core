use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

/// DO NOT TOUCH! DEPLOYED TO MAINNET
/// FIELDS PREFIXED WITH _ ARE UNUSED BUT EXIST ON MAINNET DB's
/// IF YOU WANT TO MODIFY THIS, MAKE A MIGRATION IN crates/fuel-core/src/service/genesis/importer/gas_price.rs
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V0Metadata {
    /// The gas price to cover the execution of the next block
    pub new_exec_price: u64,
    // Execution
    /// The lowest the algorithm allows the exec gas price to go
    pub _min_exec_gas_price: u64,
    /// The Percentage the execution gas price will change in a single block, either increase or decrease
    /// based on the fullness of the last L2 block
    pub _exec_gas_price_change_percent: u64,
    /// The height for which the `new_exec_price` is calculated, which should be the _next_ block
    pub l2_block_height: u32,
    /// The threshold of gas usage above and below which the gas price will increase or decrease
    /// This is a percentage of the total capacity of the L2 block
    pub _l2_block_fullness_threshold_percent: u64,
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
            _min_exec_gas_price: updater.min_exec_gas_price,
            _exec_gas_price_change_percent: updater.exec_gas_price_change_percent,
            _l2_block_fullness_threshold_percent: updater
                .l2_block_fullness_threshold_percent,
        }
    }
}