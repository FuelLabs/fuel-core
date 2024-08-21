use clap::Parser;
use fuel_core::chain_config::{
    ChainConfig,
    ConsensusConfig,
    PoAV2,
    SnapshotWriter,
    StateConfig,
};
use fuel_core_bin::cli::snapshot;
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::{
        message::MessageStatus,
        TransactionStatus,
    },
};
use fuel_core_types::{
    blockchain::header::LATEST_STATE_TRANSITION_VERSION,
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_crypto::PublicKey,
    fuel_merkle::binary,
    fuel_tx::*,
    fuel_vm::*,
};
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    collections::BTreeMap,
    ops::Deref,
    path::PathBuf,
};
use tempfile::{
    tempdir,
    TempDir,
};
use test_helpers::{
    fuel_core_driver::FuelCoreDriver,
    produce_block_with_tx,
};

async fn take_snapshot(db_dir: &TempDir, snapshot_dir: &TempDir) -> anyhow::Result<()> {
    snapshot::exec(snapshot::Command::parse_from([
        "_IGNORED_",
        "--db-path",
        db_dir.path().to_str().unwrap(),
        "--output-directory",
        snapshot_dir.path().to_str().unwrap(),
        "everything",
        "encoding",
        "parquet",
    ]))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_old_blocks_are_preserved() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);

    let core =
        FuelCoreDriver::spawn_feeless(&["--debug", "--poa-instant", "true"]).await?;
    // Add some blocks
    produce_block_with_tx(&mut rng, &core.client).await;
    produce_block_with_tx(&mut rng, &core.client).await;
    let original_blocks = core
        .client
        .blocks(PaginationRequest {
            cursor: None,
            results: 100,
            direction: PageDirection::Forward,
        })
        .await
        .expect("Failed to get blocks")
        .results;
    // Stop the node, keep the db
    let db_dir = core.kill().await;
    assert_eq!(original_blocks.len(), 3);

    // ------------------------- The genesis node is stopped -------------------------

    // Take a snapshot
    let snapshot_dir = tempdir().expect("Failed to create temp dir");
    take_snapshot(&db_dir, &snapshot_dir)
        .await
        .expect("Failed to take first snapshot");

    // ------------------------- Start a node with the first regenesis -------------------------

    // Start a new node with the snapshot
    let core = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        snapshot_dir.path().to_str().unwrap(),
    ])
    .await?;

    produce_block_with_tx(&mut rng, &core.client).await;
    let regenesis_blocks = core
        .client
        .blocks(PaginationRequest {
            cursor: None,
            results: 100,
            direction: PageDirection::Forward,
        })
        .await
        .expect("Failed to get blocks")
        .results;

    // Stop the node, keep the db
    let db_dir = core.kill().await;
    // We should have generated one new genesis block and one new generated block
    assert_eq!(original_blocks.len() + 2, regenesis_blocks.len());

    // ------------------------- Stop a node with the first regenesis -------------------------

    // Take a snapshot a new snapshot and perform the second regenesis
    let snapshot_dir = tempdir().expect("Failed to create temp dir");
    take_snapshot(&db_dir, &snapshot_dir)
        .await
        .expect("Failed to take second snapshot");

    // ------------------------- Start a node with the second regenesis -------------------------

    // Make sure the old blocks persisted through the second regenesis
    let core = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        snapshot_dir.path().to_str().unwrap(),
    ])
    .await?;

    produce_block_with_tx(&mut rng, &core.client).await;
    // When
    let regenesis_blocks = core
        .client
        .blocks(PaginationRequest {
            cursor: None,
            results: 100,
            direction: PageDirection::Forward,
        })
        .await
        .expect("Failed to get blocks")
        .results;

    // Then
    // We should have generated one new genesis block and one new generated block,
    // but the old ones should be the same.
    assert_eq!(original_blocks.len() + 4, regenesis_blocks.len());
    assert_eq!(original_blocks[0], regenesis_blocks[0]);
    assert_eq!(original_blocks[1], regenesis_blocks[1]);
    assert_eq!(original_blocks[2], regenesis_blocks[2]);

    for block in &regenesis_blocks {
        // When
        let result = core.client.block(&block.id).await;

        // Then
        result
            .expect("Requested block successfully")
            .expect("The block and all related data should migrate");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_spent_messages_are_preserved() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let state_config_dir = tempdir().expect("Failed to create temp dir");
    let nonce = [123; 32].into();
    let state_config = fuel_core::chain_config::StateConfig {
        messages: vec![fuel_core::chain_config::MessageConfig {
            sender: rng.gen(),
            recipient: rng.gen(),
            nonce,
            amount: 123,
            data: vec![],
            da_height: Default::default(),
        }],
        ..Default::default()
    };
    let writer = SnapshotWriter::json(state_config_dir.path());
    writer
        .write_state_config(state_config, &ChainConfig::local_testnet())
        .unwrap();

    let core = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        state_config_dir.path().to_str().unwrap(),
    ])
    .await?;

    // Add some blocks
    let secret = SecretKey::random(&mut rng);
    let tx_with_message = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_message_input(
            secret,
            rng.gen(),
            nonce,
            Default::default(),
            Default::default(),
        )
        .add_output(Output::change(
            Default::default(),
            Default::default(),
            Default::default(),
        ))
        .finalize_as_transaction();
    core.client
        .submit_and_await_commit(&tx_with_message)
        .await
        .unwrap();

    let status = core
        .client
        .message_status(&nonce)
        .await
        .expect("Failed to get message status");
    assert_eq!(status, MessageStatus::Spent);

    // Stop the node, keep the db
    let db_dir = core.kill().await;

    // ------------------------- The genesis node is stopped -------------------------

    // Take a snapshot
    let snapshot_dir = tempdir().expect("Failed to create temp dir");
    take_snapshot(&db_dir, &snapshot_dir)
        .await
        .expect("Failed to take first snapshot");

    // ------------------------- Start a node with the regenesis -------------------------

    // Start a new node with the snapshot
    let core = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        snapshot_dir.path().to_str().unwrap(),
    ])
    .await?;

    let status = core
        .client
        .message_status(&nonce)
        .await
        .expect("Failed to get message status");
    assert_eq!(status, MessageStatus::Spent);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_processed_transactions_are_preserved() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let core =
        FuelCoreDriver::spawn_feeless(&["--debug", "--poa-instant", "true"]).await?;

    // Add some blocks
    let secret = SecretKey::random(&mut rng);
    let tx = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_coin_input(
            secret,
            rng.gen(),
            1234,
            Default::default(),
            Default::default(),
        )
        .add_output(Output::change(
            Default::default(),
            Default::default(),
            Default::default(),
        ))
        .finalize_as_transaction();
    core.client.submit_and_await_commit(&tx).await.unwrap();

    let TransactionStatus::SqueezedOut { reason } =
        core.client.submit_and_await_commit(&tx).await.unwrap()
    else {
        panic!("Expected transaction to be squeezed out")
    };
    assert!(reason.contains("Transaction id was already used"));

    // Stop the node, keep the db
    let db_dir = core.kill().await;

    // ------------------------- The genesis node is stopped -------------------------

    // Take a snapshot
    let snapshot_dir = tempdir().expect("Failed to create temp dir");
    take_snapshot(&db_dir, &snapshot_dir)
        .await
        .expect("Failed to take first snapshot");

    // ------------------------- Start a node with the regenesis -------------------------

    // Start a new node with the snapshot
    let core = FuelCoreDriver::spawn_feeless(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        snapshot_dir.path().to_str().unwrap(),
    ])
    .await?;

    let TransactionStatus::SqueezedOut { reason } =
        core.client.submit_and_await_commit(&tx).await.unwrap()
    else {
        panic!("Expected transaction to be squeezed out")
    };
    assert!(
        reason.contains("Transaction id was already used"),
        "Unexpected message {reason:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_message_proofs_are_preserved() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let core =
        FuelCoreDriver::spawn_feeless(&["--debug", "--poa-instant", "true"]).await?;
    let base_asset_id = *core
        .node
        .shared
        .config
        .snapshot_reader
        .chain_config()
        .consensus_parameters
        .base_asset_id();

    let secret = SecretKey::random(&mut rng);
    let public_key: PublicKey = (&secret).into();
    let address = Input::owner(&public_key);
    let script_data = [address.to_vec(), 100u64.to_be_bytes().to_vec()]
        .into_iter()
        .flatten()
        .collect_vec();
    let script: Vec<u8> = vec![
        op::gtf(0x10, 0x00, GTFArgs::ScriptData.into()),
        op::addi(0x11, 0x10, Bytes32::LEN as u16),
        op::lw(0x11, 0x11, 0),
        op::smo(0x10, 0x00, 0x00, 0x11),
        op::ret(RegId::ONE),
    ]
    .into_iter()
    .collect();

    // Given
    let tx = TransactionBuilder::script(script, script_data)
        .add_unsigned_coin_input(
            secret,
            rng.gen(),
            100_000_000,
            base_asset_id,
            Default::default(),
        )
        .add_output(Output::change(
            Default::default(),
            Default::default(),
            base_asset_id,
        ))
        .script_gas_limit(1_000_000)
        .finalize_as_transaction();

    let tx_id = tx.id(&Default::default());

    core.client.submit_and_await_commit(&tx).await.unwrap();
    let message_block = core.client.chain_info().await.unwrap().latest_block;
    let message_block_height = message_block.header.height;

    core.client.produce_blocks(10, None).await.unwrap();

    let receipts = core.client.receipts(&tx_id).await.unwrap().unwrap();
    let nonces: Vec<_> = receipts.iter().filter_map(|r| r.nonce()).collect();
    let nonce = nonces[0];

    let proof = core
        .client
        .message_proof(&tx_id, nonce, None, Some((message_block_height + 1).into()))
        .await
        .expect("Unable to get message proof")
        .expect("Message proof not found");
    let prev_root = proof.commit_block_header.prev_root;
    let block_proof_index = proof.block_proof.proof_index;
    let block_proof_set: Vec<_> = proof
        .block_proof
        .proof_set
        .iter()
        .map(|bytes| *bytes.deref())
        .collect();
    assert!(binary::verify(
        &prev_root,
        &proof.message_block_header.id,
        &block_proof_set,
        block_proof_index,
        proof.commit_block_header.height as u64,
    ));

    // When
    let db_dir = core.kill().await;

    // ------------------------- The genesis node is stopped -------------------------

    // Take a snapshot
    let snapshot_dir = tempdir().expect("Failed to create temp dir");
    take_snapshot(&db_dir, &snapshot_dir)
        .await
        .expect("Failed to take first snapshot");

    // ------------------------- Start a node with the regenesis -------------------------

    // Regenesis increases the version of the executor by one.
    // We want to use native execution to produce blocks,
    // so we override the version of the native executor.
    let latest_state_transition_version = LATEST_STATE_TRANSITION_VERSION
        .saturating_add(1)
        .to_string();
    let core = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        snapshot_dir.path().to_str().unwrap(),
        "--native-executor-version",
        latest_state_transition_version.as_str(),
    ])
    .await?;

    core.client.produce_blocks(10, None).await.unwrap();
    let latest_block = core.client.chain_info().await.unwrap().latest_block;
    let receipts = core.client.receipts(&tx_id).await.unwrap().unwrap();
    let nonces: Vec<_> = receipts.iter().filter_map(|r| r.nonce()).collect();
    let nonce = nonces[0];

    for block_height in message_block_height + 1..latest_block.header.height {
        let proof = core
            .client
            .message_proof(&tx_id, nonce, None, Some(block_height.into()))
            .await
            .expect("Unable to get message proof")
            .expect("Message proof not found");
        let prev_root = proof.commit_block_header.prev_root;
        let block_proof_set: Vec<_> = proof
            .block_proof
            .proof_set
            .iter()
            .map(|bytes| *bytes.deref())
            .collect();
        let block_proof_index = proof.block_proof.proof_index;
        let message_block_id = proof.message_block_header.id;
        let count = proof.commit_block_header.height as u64;
        assert!(binary::verify(
            &prev_root,
            &message_block_id,
            &block_proof_set,
            block_proof_index,
            count,
        ));
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_same_chain_config_keeps_genesis() -> anyhow::Result<()> {
    let state_config = StateConfig::local_testnet();
    let original_chain_config = ChainConfig::local_testnet();
    let tmp_dir = tempdir()?;
    let tmp_path: PathBuf = tmp_dir.path().into();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer.write_state_config(state_config.clone(), &original_chain_config)?;

    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
        ],
    )
    .await?;

    // Given
    let original_consensus = core
        .client
        .block_by_height(0u32.into())
        .await
        .expect("Failed to get blocks")
        .expect("Genesis block should exists")
        .consensus;
    let tmp_dir = core.kill().await;

    // When
    let non_modified_chain_config = original_chain_config.clone();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer
        .write_state_config(state_config.clone(), &non_modified_chain_config)?;
    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
        ],
    )
    .await?;

    // Then
    let non_modified_consensus = core
        .client
        .block_by_height(0u32.into())
        .await
        .expect("Failed to get blocks")
        .expect("Genesis block should exists")
        .consensus;
    assert_eq!(original_consensus, non_modified_consensus);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_new_chain_config_updates_genesis() -> anyhow::Result<()> {
    let state_config = StateConfig::local_testnet();
    let original_chain_config = ChainConfig::local_testnet();
    let tmp_dir = tempdir()?;
    let tmp_path: PathBuf = tmp_dir.path().into();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer.write_state_config(state_config.clone(), &original_chain_config)?;

    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
        ],
    )
    .await?;

    // Given
    let original_consensus = core
        .client
        .block_by_height(0u32.into())
        .await
        .expect("Failed to get blocks")
        .expect("Genesis block should exists")
        .consensus;
    let tmp_dir = core.kill().await;

    // When
    let mut modified_chain_config = original_chain_config.clone();
    modified_chain_config.chain_name = "New name of the chain".to_string();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer.write_state_config(state_config.clone(), &modified_chain_config)?;
    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
        ],
    )
    .await?;

    // Then
    let modified_consensus = core
        .client
        .block_by_height(0u32.into())
        .await
        .expect("Failed to get blocks")
        .expect("Genesis block should exists")
        .consensus;
    assert_ne!(original_consensus, modified_consensus);

    Ok(())
}

fn random_key(rng: &mut StdRng) -> (SecretKey, Address) {
    let secret = SecretKey::random(rng);
    let pk = secret.public_key();
    let owner = Input::owner(&pk);
    (secret, owner)
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_overwritten_old_poa_key_doesnt_rollback_the_state(
) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let state_config = StateConfig::local_testnet();
    let mut original_chain_config = ChainConfig::local_testnet();
    let tmp_dir = tempdir()?;
    let tmp_path: PathBuf = tmp_dir.path().into();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());

    let (original_secret_key, original_address) = random_key(&mut rng);
    original_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, Default::default()));
    snapshot_writer.write_state_config(state_config.clone(), &original_chain_config)?;

    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            original_secret_key.to_string().as_str(),
        ],
    )
    .await?;

    // Given
    core.client.produce_blocks(10, None).await?;
    let original_block_height = core
        .client
        .chain_info()
        .await
        .expect("Failed to get blocks")
        .latest_block
        .header
        .height;
    let tmp_dir = core.kill().await;

    // When
    let override_height = 5u32;
    let mut modified_chain_config = original_chain_config.clone();
    modified_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, {
            let mut overrides = BTreeMap::default();
            overrides.insert(override_height.into(), original_address);
            overrides
        }));

    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer.write_state_config(state_config.clone(), &modified_chain_config)?;
    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            original_secret_key.to_string().as_str(),
        ],
    )
    .await?;

    // Then
    let block_height_after_override = core
        .client
        .chain_info()
        .await
        .expect("Failed to get blocks")
        .latest_block
        .header
        .height;
    assert_eq!(original_block_height, block_height_after_override);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_empty_node_with_overwritten_poa_works() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let state_config = StateConfig::local_testnet();
    let mut original_chain_config = ChainConfig::local_testnet();
    let tmp_dir = tempdir()?;
    let tmp_path: PathBuf = tmp_dir.path().into();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());

    // Given
    let (original_secret_key, original_address) = random_key(&mut rng);
    original_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, {
            let mut overrides = BTreeMap::default();
            overrides.insert(123.into(), original_address);
            overrides
        }));
    snapshot_writer.write_state_config(state_config.clone(), &original_chain_config)?;

    // When
    let result = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            original_secret_key.to_string().as_str(),
        ],
    )
    .await;

    // Then
    let core = result.expect("Failed to start the node");
    produce_block_with_tx(&mut rng, &core.client).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_overwritten_new_poa_key_rollbacks_the_state(
) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let state_config = StateConfig::local_testnet();
    let mut original_chain_config = ChainConfig::local_testnet();
    let tmp_dir = tempdir()?;
    let tmp_path: PathBuf = tmp_dir.path().into();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());

    let (original_secret_key, original_address) = random_key(&mut rng);
    original_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, Default::default()));
    snapshot_writer.write_state_config(state_config.clone(), &original_chain_config)?;

    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            original_secret_key.to_string().as_str(),
        ],
    )
    .await?;

    // Given
    core.client.produce_blocks(10, None).await?;
    let original_block_height = core
        .client
        .chain_info()
        .await
        .expect("Failed to get blocks")
        .latest_block
        .header
        .height;
    let tmp_dir = core.kill().await;

    // When
    let override_height = 5u32;
    let (new_secret_key, new_address) = random_key(&mut rng);
    let mut modified_chain_config = original_chain_config.clone();
    modified_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, {
            let mut overrides = BTreeMap::default();
            overrides.insert(override_height.into(), new_address);
            overrides
        }));

    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer.write_state_config(state_config.clone(), &modified_chain_config)?;
    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            new_secret_key.to_string().as_str(),
        ],
    )
    .await?;

    // Then
    let block_height_after_override = core
        .client
        .chain_info()
        .await
        .expect("Failed to get blocks")
        .latest_block
        .header
        .height;
    assert_ne!(original_block_height, block_height_after_override);
    assert_eq!(override_height - 1, block_height_after_override);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_node_with_overwritten_new_poa_key_from_the_future_doesnt_rollback_the_state(
) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(1234);
    let state_config = StateConfig::local_testnet();
    let mut original_chain_config = ChainConfig::local_testnet();
    let tmp_dir = tempdir()?;
    let tmp_path: PathBuf = tmp_dir.path().into();
    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());

    let (original_secret_key, original_address) = random_key(&mut rng);
    original_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, Default::default()));
    snapshot_writer.write_state_config(state_config.clone(), &original_chain_config)?;

    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            original_secret_key.to_string().as_str(),
        ],
    )
    .await?;

    // Given
    core.client.produce_blocks(10, None).await?;
    let original_block_height = core
        .client
        .chain_info()
        .await
        .expect("Failed to get blocks")
        .latest_block
        .header
        .height;
    let tmp_dir = core.kill().await;

    // When
    let override_height = original_block_height + 5u32;
    let (new_secret_key, new_address) = random_key(&mut rng);
    let mut modified_chain_config = original_chain_config.clone();
    modified_chain_config.consensus =
        ConsensusConfig::PoAV2(PoAV2::new(original_address, {
            let mut overrides = BTreeMap::default();
            overrides.insert(override_height.into(), new_address);
            overrides
        }));

    let snapshot_writer = SnapshotWriter::json(tmp_path.clone());
    snapshot_writer.write_state_config(state_config.clone(), &modified_chain_config)?;
    let core = FuelCoreDriver::spawn_with_directory(
        tmp_dir,
        &[
            "--debug",
            "--poa-instant",
            "true",
            "--snapshot",
            tmp_path.to_str().unwrap(),
            "--consensus-key",
            new_secret_key.to_string().as_str(),
        ],
    )
    .await?;

    // Then
    let block_height_after_override = core
        .client
        .chain_info()
        .await
        .expect("Failed to get blocks")
        .latest_block
        .header
        .height;
    assert_eq!(original_block_height, block_height_after_override);

    Ok(())
}
