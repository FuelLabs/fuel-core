use fuel_core_types::fuel_tx::{
    ConsensusParameters,
    ContractId,
};

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Network-wide common parameters used for validating the chain
    pub consensus_parameters: ConsensusParameters,
    /// The `ContractId` of the fee recipient.
    pub coinbase_recipient: ContractId,
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,
    /// Default mode for utxo_validation
    pub utxo_validation_default: bool,
}
