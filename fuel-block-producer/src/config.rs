use fuel_core_interfaces::common::fuel_tx::Address;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub utxo_validation: bool,
    pub coinbase_recipient: Address,
    pub metrics: bool,
}
