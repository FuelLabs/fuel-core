#![allow(non_snake_case)]

use crate::service::adapters::{
    consensus_parameters_provider,
    gas_price_adapters::GasPriceSettings,
};
use fuel_core_services::SharedMutex;
use std::{
    collections::HashMap,
    sync::Arc,
};

#[test]
fn settings__can_retrieve_settings() {
    // given
    let param_version =
        fuel_core_types::blockchain::header::ConsensusParametersVersion::default();
    let params =
        fuel_core_types::fuel_tx::consensus_parameters::ConsensusParametersV1::default();
    let mut hash_map = HashMap::new();
    hash_map.insert(param_version, Arc::new(params.clone().into()));
    let shared_state = consensus_parameters_provider::SharedState {
        latest_consensus_parameters_version: SharedMutex::new(param_version),
        consensus_parameters: SharedMutex::new(hash_map),
        database: Default::default(),
    };
    let consensus_parameters_provider =
        crate::service::adapters::ConsensusParametersProvider::new(shared_state);
    // when
    let actual =
        crate::service::adapters::gas_price_adapters::GasPriceSettingsProvider::settings(
            &consensus_parameters_provider,
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
