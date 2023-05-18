use crate::client::schema;

#[derive(Clone, Debug)]
pub struct ConsensusParameters {
    pub contract_max_size: u64,
    pub max_inputs: u64,
    pub max_outputs: u64,
    pub max_witnesses: u64,
    pub max_gas_per_tx: u64,
    pub max_script_length: u64,
    pub max_script_data_length: u64,
    pub max_storage_slots: u64,
    pub max_predicate_length: u64,
    pub max_predicate_data_length: u64,
    pub gas_price_factor: u64,
    pub gas_per_byte: u64,
    pub max_message_data_length: u64,
    pub chain_id: u64,
}

// GraphQL Translation

impl From<ConsensusParameters> for fuel_core_types::fuel_tx::ConsensusParameters {
    fn from(value: ConsensusParameters) -> Self {
        Self {
            contract_max_size: value.contract_max_size,
            max_inputs: value.max_inputs,
            max_outputs: value.max_outputs,
            max_witnesses: value.max_witnesses,
            max_gas_per_tx: value.max_gas_per_tx,
            max_script_length: value.max_script_length,
            max_script_data_length: value.max_script_data_length,
            max_storage_slots: value.max_storage_slots,
            max_predicate_length: value.max_predicate_length,
            max_predicate_data_length: value.max_predicate_data_length,
            gas_price_factor: value.gas_price_factor,
            gas_per_byte: value.gas_per_byte,
            max_message_data_length: value.max_message_data_length,
            chain_id: value.chain_id,
        }
    }
}

impl From<schema::chain::ConsensusParameters> for ConsensusParameters {
    fn from(value: schema::chain::ConsensusParameters) -> Self {
        Self {
            contract_max_size: value.contract_max_size.into(),
            max_inputs: value.max_inputs.into(),
            max_outputs: value.max_outputs.into(),
            max_witnesses: value.max_witnesses.into(),
            max_gas_per_tx: value.max_gas_per_tx.into(),
            max_script_length: value.max_script_length.into(),
            max_script_data_length: value.max_script_data_length.into(),
            max_storage_slots: value.max_storage_slots.into(),
            max_predicate_length: value.max_predicate_length.into(),
            max_predicate_data_length: value.max_predicate_data_length.into(),
            gas_price_factor: value.gas_price_factor.into(),
            gas_per_byte: value.gas_per_byte.into(),
            max_message_data_length: value.max_message_data_length.into(),
            chain_id: value.chain_id.into(),
        }
    }
}
