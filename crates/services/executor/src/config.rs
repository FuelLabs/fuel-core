use fuel_core_types::{
    fuel_tx::{
        Address,
        ConsensusParameters,
    },
    fuel_vm::GasCosts,
};

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Network-wide common parameters used for validating the chain
    pub transaction_parameters: ConsensusParameters,
    /// The address of the fee recipient
    pub coinbase_recipient: Address,
    /// The cost schedule of various ops
    pub gas_costs: GasCosts,
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,
    /// Default mode for utxo_validation
    pub utxo_validation_default: bool,
}
