use fuel_core::chain_config::ChainConfig;
use fuel_core_types::fuel_tx::GasCosts;

#[test_case::test_case(
    "./../deployment/scripts/chainspec/beta";
    "Beta chainconfig"
)]
#[test_case::test_case(
    "./../deployment/scripts/chainspec/dev";
    "Dev chainconfig" 
)]
fn test_deployment_chainconfig(path: &str) {
    let chain_config = ChainConfig::load_from_directory(path)
        .expect("Should be able to load chain config");

    // TODO: add state config check in subsequent PR

    let benchmark_gas_costs =
        GasCosts::new(fuel_core_benches::default_gas_costs::default_gas_costs());

    // Deployment configuration should use gas costs from benchmarks.
    assert_eq!(chain_config.consensus_parameters.gas_costs, benchmark_gas_costs);
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
