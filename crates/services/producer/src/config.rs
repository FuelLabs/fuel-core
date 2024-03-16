use fuel_core_types::fuel_types::ContractId;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub utxo_validation: bool,
    pub coinbase_recipient: Option<ContractId>,
    pub metrics: bool,
}
