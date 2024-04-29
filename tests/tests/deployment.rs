use std::{
    env,
    path::Path,
};

use fuel_core::chain_config::{
    ChainConfig,
    SnapshotMetadata,
    SnapshotWriter,
    StateConfig,
    TableEncoding,
};
use fuel_core_types::fuel_tx::GasCosts;
use fuel_core_upgradable_executor::WASM_BYTECODE;

#[allow(irrefutable_let_patterns)]
#[test_case::test_case( "./../bin/fuel-core/chainspec/testnet" ; "Beta chainconfig" )]
#[test_case::test_case( "./../bin/fuel-core/chainspec/dev-testnet" ; "Dev chainconfig"  )]
fn test_deployment_chainconfig(path: impl AsRef<Path>) -> anyhow::Result<()> {
    let path = path.as_ref();
    let stored_snapshot = SnapshotMetadata::read(path).unwrap();
    let mut chain_config = ChainConfig::from_snapshot_metadata(&stored_snapshot)?;
    let state_config = StateConfig::from_snapshot_metadata(stored_snapshot.clone())?;

    // Deployment configuration should use gas costs from benchmarks.
    let benchmark_gas_costs =
        GasCosts::new(fuel_core_benches::default_gas_costs::default_gas_costs());
    chain_config
        .consensus_parameters
        .set_gas_costs(benchmark_gas_costs);

    if env::var_os("OVERRIDE_CHAIN_CONFIGS").is_some() {
        chain_config.state_transition_bytecode = WASM_BYTECODE.to_vec();
        std::fs::remove_file(&stored_snapshot.chain_config)?;
        chain_config.write(&stored_snapshot.chain_config)?;
    }

    let temp_dir = tempfile::tempdir()?;
    let writer = SnapshotWriter::json(temp_dir.path());
    let generated_snapshot = writer.write_state_config(state_config, &chain_config)?;

    let chain_config = std::fs::read_to_string(generated_snapshot.chain_config)?
        .trim()
        .to_string();
    let stored_chain_config = std::fs::read_to_string(stored_snapshot.chain_config)?
        .trim()
        .to_string();
    assert_eq!(
        chain_config, stored_chain_config,
        "Chain config should match the one in the deployment directory"
    );

    let stored_state_config = {
        let TableEncoding::Json {
            filepath: stored_state,
        } = stored_snapshot.table_encoding
        else {
            panic!("State encoding should be JSON")
        };
        std::fs::read_to_string(stored_state)?.trim().to_string()
    };
    let generated_state_config = {
        let TableEncoding::Json {
            filepath: generated_state,
        } = generated_snapshot.table_encoding
        else {
            panic!("State encoding should be JSON")
        };
        std::fs::read_to_string(generated_state)?.trim().to_string()
    };
    assert_eq!(
        stored_state_config, generated_state_config,
        "State config should match the one in the deployment directory"
    );

    Ok(())
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
