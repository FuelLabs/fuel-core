use std::path::Path;

use fuel_core::chain_config::{
    ChainConfig,
    StateConfig,
    StateWriter,
    CHAIN_CONFIG_FILENAME,
    STATE_CONFIG_FILENAME,
};
use fuel_core_types::fuel_tx::GasCosts;

#[test_case::test_case( "./../deployment/scripts/chainspec/beta" ; "Beta chainconfig" )]
#[test_case::test_case( "./../deployment/scripts/chainspec/dev" ; "Dev chainconfig"  )]
fn test_deployment_chainconfig(path: impl AsRef<Path>) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut chain_config = ChainConfig::load_from_snapshot(path)?;
    let state_config = StateConfig::load_from_snapshot(path)?;

    // Deployment configuration should use gas costs from benchmarks.
    let benchmark_gas_costs =
        GasCosts::new(fuel_core_benches::default_gas_costs::default_gas_costs());
    chain_config.consensus_parameters.gas_costs = benchmark_gas_costs;

    let temp_dir = tempfile::tempdir()?;
    StateWriter::json(temp_dir.path()).write(state_config)?;

    let chain_config_bytes = serde_json::to_string_pretty(&chain_config)?;
    let stored_chain_config_bytes =
        std::fs::read_to_string(path.join(CHAIN_CONFIG_FILENAME))?;
    assert_eq!(
        chain_config_bytes, stored_chain_config_bytes,
        "Chain config should match the one in the deployment directory"
    );

    let stored_state_config_bytes =
        std::fs::read_to_string(path.join(STATE_CONFIG_FILENAME))?;
    let state_config_bytes =
        std::fs::read_to_string(temp_dir.path().join(STATE_CONFIG_FILENAME))?;
    assert_eq!(
        stored_state_config_bytes, state_config_bytes,
        "State config should match the one in the deployment directory"
    );

    Ok(())
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
