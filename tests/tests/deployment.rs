use std::path::Path;

use fuel_core::chain_config::{
    ChainConfig,
    SnapshotMetadata,
    StateConfig,
    StateEncoding,
    StateWriter,
};
use fuel_core_types::fuel_tx::GasCosts;

#[allow(irrefutable_let_patterns)]
#[test_case::test_case( "./../deployment/scripts/chainspec/testnet" ; "Beta chainconfig" )]
#[test_case::test_case( "./../deployment/scripts/chainspec/dev-testnet" ; "Dev chainconfig"  )]
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

    let temp_dir = tempfile::tempdir()?;
    let generated_snapshot = SnapshotMetadata::write_json(temp_dir.path())?;
    chain_config.write(generated_snapshot.chain_config())?;
    StateWriter::for_snapshot(&generated_snapshot)?.write(state_config)?;

    let chain_config_bytes = std::fs::read_to_string(generated_snapshot.chain_config())?;
    let stored_chain_config = std::fs::read_to_string(stored_snapshot.chain_config())?;
    pretty_assertions::assert_eq!(
        chain_config_bytes,
        stored_chain_config,
        "Chain config should match the one in the deployment directory"
    );

    let stored_state_config = {
        let StateEncoding::Json {
            filepath: stored_state,
        } = stored_snapshot.state_encoding()
        else {
            panic!("State encoding should be JSON")
        };
        std::fs::read_to_string(stored_state)?
    };
    let generated_state_config = {
        let StateEncoding::Json {
            filepath: generated_state,
        } = generated_snapshot.state_encoding()
        else {
            panic!("State encoding should be JSON")
        };
        std::fs::read_to_string(generated_state)?
    };
    pretty_assertions::assert_eq!(
        stored_state_config,
        generated_state_config,
        "State config should match the one in the deployment directory"
    );

    Ok(())
}

/// This dummy test allows to run tests from IDE in this file.
#[test]
#[ignore]
fn dummy() {}
