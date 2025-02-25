#![allow(non_snake_case)]

use crate::service::adapters::{
    chain_state_info_provider,
    gas_price_adapters::GasPriceSettings,
};
use fuel_core_services::SharedRwLock;
use std::{
    collections::HashMap,
    sync::Arc,
};

#[test]
fn settings__can_retrieve_settings() {
    // given
    let param_version =
        fuel_core_types::blockchain::header::ConsensusParametersVersion::default();
    let stf_version =
        fuel_core_types::blockchain::header::StateTransitionBytecodeVersion::default();
    let params =
        fuel_core_types::fuel_tx::consensus_parameters::ConsensusParametersV1::default();
    let mut hash_map = HashMap::new();
    hash_map.insert(param_version, Arc::new(params.clone().into()));
    let shared_state = chain_state_info_provider::SharedState {
        latest_consensus_parameters_version: SharedRwLock::new(param_version),
        consensus_parameters: SharedRwLock::new(hash_map),
        database: Default::default(),
        latest_stf_version: SharedRwLock::new(stf_version),
    };
    let chain_state_info_provider =
        crate::service::adapters::ChainStateInfoProvider::new(shared_state);
    // when
    let actual =
        crate::service::adapters::gas_price_adapters::GasPriceSettingsProvider::settings(
            &chain_state_info_provider,
            &param_version,
        )
        .unwrap();
    // then
    let expected = GasPriceSettings {
        gas_price_factor: params.fee_params.gas_price_factor(),
        block_gas_limit: params.block_gas_limit,
    };
    assert_eq!(expected, actual);
}
