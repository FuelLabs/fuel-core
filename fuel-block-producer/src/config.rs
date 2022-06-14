use fuel_core_interfaces::common::fuel_types::{Address, Word};

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The primary identity associated with the validator (i.e. staking key)
    pub validator_id: Address,
    pub max_gas_per_block: Word,
}
