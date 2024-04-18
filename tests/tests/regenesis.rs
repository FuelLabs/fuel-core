use clap::Parser;
use fuel_core::{
    chain_config::{
        ChainConfig,
        SnapshotWriter,
    },
    service::{
        FuelService,
        ServiceTrait,
    },
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
    FuelClient,
};
use fuel_core_types::{
    fuel_tx::*,
    fuel_vm::*,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use tempfile::{
    tempdir,
    TempDir,
};

pub struct FuelCoreDriver {
    /// This must be before the db_dir as the drop order matters here
    pub node: FuelService,
    pub db_dir: TempDir,
    pub client: FuelClient,
}
impl FuelCoreDriver {
    pub async fn spawn(extra_args: &[&str]) -> anyhow::Result<Self> {
        // Generate temp params
        let db_dir = tempdir()?;

        let mut args = vec![
            "_IGNORED_",
            "--db-path",
            db_dir.path().to_str().unwrap(),
            "--port",
            "0",
        ];
        args.extend(extra_args);

        let node = fuel_core_bin::cli::run::get_service(
            fuel_core_bin::cli::run::Command::parse_from(args),
        )?;

        node.start_and_await().await?;

        let client = FuelClient::from(node.shared.graph_ql.bound_address);
        Ok(Self {
            node,
            db_dir,
            client,
        })
    }

    /// Stops the node, returning the db only
    /// Ignoring the return value drops the db as well.
    pub async fn kill(self) -> TempDir {
        println!("Stopping fuel service");
        self.node
            .stop_and_await()
            .await
            .expect("Failed to stop the node");
        self.db_dir
    }
}

async fn produce_block_with_tx(rng: &mut StdRng, client: &FuelClient) {
    let secret = SecretKey::random(rng);
    let contract_tx = TransactionBuilder::script(vec![], vec![])
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
    client.submit_and_await_commit(&contract_tx).await.unwrap();
}

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

    let core = FuelCoreDriver::spawn(&["--debug", "--poa-instant", "true"]).await?;

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
    let core = FuelCoreDriver::spawn(&[
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
    let core = FuelCoreDriver::spawn(&[
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

    // We should have generated one new genesis block and one new generated block,
    // but the old ones should be the same.
    assert_eq!(original_blocks.len() + 4, regenesis_blocks.len());
    assert_eq!(original_blocks[0], regenesis_blocks[0]);
    assert_eq!(original_blocks[1], regenesis_blocks[1]);
    assert_eq!(original_blocks[2], regenesis_blocks[2]);

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

    let core = FuelCoreDriver::spawn(&[
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
    let core = FuelCoreDriver::spawn(&[
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
    let core = FuelCoreDriver::spawn(&["--debug", "--poa-instant", "true"]).await?;

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
    let core = FuelCoreDriver::spawn(&[
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
    assert!(reason.contains("Transaction id was already used"));

    Ok(())
}
