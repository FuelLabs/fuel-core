use crate::client::{
    schema,
    types::primitives::ChainId,
};

use crate::client::types::GasCosts;

#[derive(Clone, Debug)]
pub struct ConsensusParameters {
    pub tx_params: TxParameters,
    pub predicate_params: PredicateParameters,
    pub script_params: ScriptParameters,
    pub contract_params: ContractParameters,
    pub fee_params: FeeParameters,
    pub chain_id: ChainId,
    pub gas_costs: GasCosts,
}

// GraphQL Translation

impl From<ConsensusParameters> for fuel_core_types::fuel_tx::ConsensusParameters {
    fn from(value: ConsensusParameters) -> Self {
        Self {
            tx_params: value.tx_params.into(),
            predicate_params: value.predicate_params.into(),
            script_params: value.script_params.into(),
            contract_params: value.contract_params.into(),
            fee_params: value.fee_params.into(),
            chain_id: value.chain_id,
            gas_costs: value.gas_costs.into(),
        }
    }
}

impl From<schema::chain::ConsensusParameters> for ConsensusParameters {
    fn from(value: schema::chain::ConsensusParameters) -> Self {
        Self {
            tx_params: value.tx_params.into(),
            predicate_params: value.predicate_params.into(),
            script_params: value.script_params.into(),
            contract_params: value.contract_params.into(),
            fee_params: value.fee_params.into(),
            chain_id: value.chain_id.0.into(),
            gas_costs: value.gas_costs.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TxParameters {
    pub max_inputs: u64,
    pub max_outputs: u64,
    pub max_witnesses: u64,
    pub max_gas_per_tx: u64,
}

impl From<TxParameters> for fuel_core_types::fuel_tx::TxParameters {
    fn from(value: TxParameters) -> Self {
        Self {
            max_inputs: value.max_inputs,
            max_outputs: value.max_outputs,
            max_witnesses: value.max_witnesses,
            max_gas_per_tx: value.max_gas_per_tx,
        }
    }
}

impl From<schema::chain::TxParameters> for TxParameters {
    fn from(value: schema::chain::TxParameters) -> Self {
        Self {
            max_inputs: value.max_inputs.into(),
            max_outputs: value.max_outputs.into(),
            max_witnesses: value.max_witnesses.into(),
            max_gas_per_tx: value.max_gas_per_tx.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PredicateParameters {
    pub max_gas_per_predicate: u64,
    pub max_predicate_length: u64,
    pub max_predicate_data_length: u64,
    pub max_message_data_length: u64,
}

impl From<PredicateParameters> for fuel_core_types::fuel_tx::PredicateParameters {
    fn from(value: PredicateParameters) -> Self {
        Self {
            max_gas_per_predicate: value.max_gas_per_predicate,
            max_predicate_length: value.max_predicate_length,
            max_predicate_data_length: value.max_predicate_data_length,
            max_message_data_length: value.max_message_data_length,
        }
    }
}

impl From<schema::chain::PredicateParameters> for PredicateParameters {
    fn from(value: schema::chain::PredicateParameters) -> Self {
        Self {
            max_gas_per_predicate: value.max_gas_per_predicate.into(),
            max_predicate_length: value.max_predicate_length.into(),
            max_predicate_data_length: value.max_predicate_data_length.into(),
            max_message_data_length: value.max_message_data_length.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScriptParameters {
    pub max_script_length: u64,
    pub max_script_data_length: u64,
}

impl From<ScriptParameters> for fuel_core_types::fuel_tx::ScriptParameters {
    fn from(value: ScriptParameters) -> Self {
        Self {
            max_script_length: value.max_script_length,
            max_script_data_length: value.max_script_data_length,
        }
    }
}

impl From<schema::chain::ScriptParameters> for ScriptParameters {
    fn from(value: schema::chain::ScriptParameters) -> Self {
        Self {
            max_script_length: value.max_script_length.into(),
            max_script_data_length: value.max_script_data_length.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContractParameters {
    pub contract_max_size: u64,
    pub max_storage_slots: u64,
}

impl From<ContractParameters> for fuel_core_types::fuel_tx::ContractParameters {
    fn from(value: ContractParameters) -> Self {
        Self {
            contract_max_size: value.contract_max_size,
            max_storage_slots: value.max_storage_slots,
        }
    }
}

impl From<schema::chain::ContractParameters> for ContractParameters {
    fn from(value: schema::chain::ContractParameters) -> Self {
        Self {
            contract_max_size: value.contract_max_size.into(),
            max_storage_slots: value.max_storage_slots.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FeeParameters {
    pub gas_price_factor: u64,
    pub gas_per_byte: u64,
}

impl From<FeeParameters> for fuel_core_types::fuel_tx::FeeParameters {
    fn from(value: FeeParameters) -> Self {
        Self {
            gas_price_factor: value.gas_price_factor,
            gas_per_byte: value.gas_per_byte,
        }
    }
}

impl From<schema::chain::FeeParameters> for FeeParameters {
    fn from(value: schema::chain::FeeParameters) -> Self {
        Self {
            gas_price_factor: value.gas_price_factor.into(),
            gas_per_byte: value.gas_per_byte.into(),
        }
    }
}
