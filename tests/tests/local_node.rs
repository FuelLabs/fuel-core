use std::env;

use fuel_core::chain_config::{
    ChainConfig,
    SnapshotMetadata,
    SnapshotWriter,
    StateConfig,
    TableEncoding,
};
use fuel_core_types::{
    blockchain::header::LATEST_STATE_TRANSITION_VERSION,
    fuel_tx::GasCosts,
};
use test_helpers::fuel_core_driver::FuelCoreDriver;

#[test]
fn local_chainconfig_validity() -> anyhow::Result<()> {
    let path = "./../bin/fuel-core/chainspec/local-testnet";
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
        chain_config.genesis_state_transition_version =
            Some(LATEST_STATE_TRANSITION_VERSION);
        std::fs::remove_file(&stored_snapshot.chain_config)?;
        chain_config.write(&stored_snapshot.chain_config)?;
    } else {
        assert_eq!(
            chain_config.genesis_state_transition_version,
            Some(LATEST_STATE_TRANSITION_VERSION),
            "Genesis state transition version should match the one in the local configuration"
        );
    }

    let temp_dir = tempfile::tempdir()?;
    let writer = SnapshotWriter::json(temp_dir.path());
    let generated_snapshot = writer.write_state_config(state_config, &chain_config)?;

    let chain_config = trim_owner_lines(
        std::fs::read_to_string(generated_snapshot.chain_config)?.trim(),
    );
    let stored_chain_config =
        trim_owner_lines(std::fs::read_to_string(stored_snapshot.chain_config)?.trim());
    pretty_assertions::assert_str_eq!(
        chain_config,
        stored_chain_config,
        "Chain config should match the one in the local configuration"
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

    let stored_state_config_trimmed = trim_owner_lines(&stored_state_config);
    let generated_state_config_trimmed = trim_owner_lines(&generated_state_config);

    pretty_assertions::assert_str_eq!(
        stored_state_config_trimmed,
        generated_state_config_trimmed,
        "State config should match the one in the local configuration"
    );

    Ok(())
}

// The state config file may contain `owner_secret` entries, which are converted
// to `owner` entries when the state config is written using a snapshot writer.
// Therefore, we'll omit these from the test.
fn trim_owner_lines(s: &str) -> String {
    s.lines()
        .filter(|line| !line.contains("owner"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn start_local_node_without_any_arguments() {
    // Given
    let arguments = [];

    // When
    let service = FuelCoreDriver::spawn(&arguments).await.unwrap();

    // Then
    let block = service
        .client
        .block_by_height(0u32.into())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(block.header.height, 0u32);
    assert_eq!(
        block.header.state_transition_bytecode_version,
        LATEST_STATE_TRANSITION_VERSION
    );
    service.kill().await;
}
