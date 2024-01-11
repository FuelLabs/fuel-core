use fuel_core::chain_config::{
    ChainConfig,
    Decoder,
    SnapshotMetadata,
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
fn test_deployment_chainconfig(path: &str, chain_bytes: &str, state_bytes: &str) {
    let mut chain_config =
        ChainConfig::load(path).expect("Should be able to load chain config");
    let snapshot = SnapshotMetadata::read_from_dir(path).unwrap();
    let state_config = StateConfig::from_snapshot(snapshot)
        .expect("Should be able to load state config");

    // Deployment configuration should use gas costs from benchmarks.
    let benchmark_gas_costs =
        GasCosts::new(fuel_core_benches::default_gas_costs::default_gas_costs());
    chain_config.consensus_parameters.gas_costs = benchmark_gas_costs;

    let actual_chain_bytes = serde_json::to_string_pretty(&chain_config)
        .expect("Should be able to encode the chain config");
    let actual_state_bytes = serde_json::to_string_pretty(&state_config)
        .expect("Should be able to encode the chain config");

    assert_eq!(actual_chain_bytes, chain_bytes);
    assert_eq!(actual_state_bytes, state_bytes);
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
