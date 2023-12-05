use fuel_core::chain_config::{
    ChainConfig,
    StateConfig,
};
use fuel_core_types::fuel_tx::GasCosts;

#[test_case::test_case(
    "./../deployment/scripts/chainspec/beta",
    include_str!("../../deployment/scripts/chainspec/beta/chain_config.json"),
    include_str!("../../deployment/scripts/chainspec/beta/state_config.json");
    "Beta chainconfig"
)]
#[test_case::test_case(
    "./../deployment/scripts/chainspec/dev",
    include_str!("../../deployment/scripts/chainspec/dev/chain_config.json"),
    include_str!("../../deployment/scripts/chainspec/dev/state_config.json");
    "Dev chainconfig" 
)]
fn test_deployment_chainconfig(bytes: &str) {
    let mut chain_config: ChainConfig =
        serde_json::from_str(bytes).expect("Should be able to decode chain config");
    let benchmark_gas_costs =
        GasCosts::new(fuel_core_benches::default_gas_costs::default_gas_costs());
    // Deployment configuration should use gas costs from benchmarks.
    chain_config.consensus_parameters.gas_costs = benchmark_gas_costs;
    let actual_bytes = serde_json::to_string_pretty(&chain_config)
        .expect("Should be able to encode the chain config");

    let state_config = StateConfig::load_from_directory(dir_path)
        .expect("Should be able to load state config");
    let state_actual_bytes = serde_json::to_string_pretty(&state_config)
        .expect("Should be able to encode state config");

    assert_eq!(chain_actual_bytes, chain_bytes);
    assert_eq!(state_actual_bytes, state_bytes);
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
