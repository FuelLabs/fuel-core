use fuel_core_types::fuel_types::Address;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub utxo_validation: bool,
    pub coinbase_recipient: Address,
    pub metrics: bool,
}
