use fuel_core_types::fuel_types::{
    ChainId,
    ContractId,
};

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub utxo_validation: bool,
    pub coinbase_recipient: Option<ContractId>,
    pub chain_id: ChainId,
    pub metrics: bool,
    #[cfg(feature = "firehose")]
    pub firehose: bool,
}
