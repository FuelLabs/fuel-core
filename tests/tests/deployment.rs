use fuel_core::chain_config::ChainConfig;
use fuel_core_types::fuel_tx::GasCosts;

#[test_case::test_case(
    include_str!("../../deployment/scripts/chainspec/beta_chainspec.json"); 
    "Beta chainconfig"
)]
#[test_case::test_case(
    include_str!("../../deployment/scripts/chainspec/dev_chainspec.json"); 
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
    assert_eq!(actual_bytes, bytes);
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
