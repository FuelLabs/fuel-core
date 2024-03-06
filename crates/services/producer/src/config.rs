use fuel_core_types::fuel_types::ContractId;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub utxo_validation: bool,
    pub coinbase_recipient: Option<ContractId>,
    pub gas_price: u64,
    pub metrics: bool,
}
