use std::path::Path;

use clap::Parser;
use fuel_core::{
    chain_config::{
        SnapshotMetadata,
        SnapshotReader,
    },
    service::{
        Config,
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
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_tx::*,
    fuel_vm::{
        checked_transaction::IntoChecked,
        *,
    },
};
use rand::SeedableRng;
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
    pub async fn spawn(from_snapshot: Option<&Path>) -> anyhow::Result<Self> {
        // Generate temp params
        let db_dir = tempdir()?;

        let mut config = Config::local_node();
        config.debug = true;
        config.block_production = Trigger::Instant;
        config.combined_db_config.database_path = db_dir.path().to_path_buf();

        if let Some(path) = from_snapshot {
            let metadata = SnapshotMetadata::read(path)?;
            config.snapshot_reader = SnapshotReader::open(metadata)?;
        }

        let node = FuelService::new_node(config)
            .await
            .expect("Failed to start the node");

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

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_old_blocks_are_preserved() -> anyhow::Result<()> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1234);
    let snapshot_dir = tempdir().expect("Failed to create temp dir");

    let core = FuelCoreDriver::spawn(None).await?;

    // Add some blocks
    let secret = SecretKey::random(&mut rng);
    let contract_input = {
        let bytecode: Witness = vec![].into();
        let salt = Salt::zeroed();
        let contract = Contract::from(bytecode.as_ref());
        let code_root = contract.root();
        let balance_root = Contract::default_state_root();
        let state_root = Contract::default_state_root();

        let contract_id = contract.id(&salt, &code_root, &state_root);
        let output = Output::contract_created(contract_id, state_root);
        let create_tx = TransactionBuilder::create(bytecode, salt, vec![])
            .add_unsigned_coin_input(
                secret,
                UtxoId::new([1; 32].into(), 0),
                1234,
                Default::default(),
                Default::default(),
            )
            .add_output(output)
            .finalize_as_transaction()
            .into_checked(Default::default(), &Default::default())
            .expect("Cannot check transaction");

        let contract_input = Input::contract(
            UtxoId::new(create_tx.id(), 1),
            balance_root,
            state_root,
            Default::default(),
            contract_id,
        );

        core.client
            .submit_and_await_commit(create_tx.transaction())
            .await
            .expect("cannot insert tx into transaction pool");

        contract_input
    };
    let contract_tx = TransactionBuilder::script(vec![], vec![])
        .add_input(contract_input)
        .add_unsigned_coin_input(
            secret,
            UtxoId::new([1; 32].into(), 1),
            1234,
            Default::default(),
            Default::default(),
        )
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    core.client
        .submit_and_await_commit(&contract_tx)
        .await
        .unwrap();

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

    {
        // Stop the node, keep the db
        let db_dir = core.kill().await;

        // Take a snapshot
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
    }

    // Start a new node with the snapshot
    let core = FuelCoreDriver::spawn(Some(snapshot_dir.path())).await?;

    core.client.produce_blocks(1, None).await.unwrap();
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

    // We should have generated one new block, but the old ones should be the same
    assert_eq!(original_blocks.len() + 2, regenesis_blocks.len());
    assert_eq!(original_blocks[0], regenesis_blocks[0]);
    assert_eq!(original_blocks[1], regenesis_blocks[1]);

    let snapshot_dir = tempdir().unwrap();
    // Stop the node, keep the db
    {
        let db_dir = core.kill().await;

        // Take a snapshot again
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
    }

    // Make sure the old blocks persited through the second regenesis
    let core = FuelCoreDriver::spawn(Some(snapshot_dir.path())).await?;

    println!("Getting blocks");

    core.client.produce_blocks(1, None).await.unwrap();
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

    // We should have generated one new block, but the old ones should be the same
    assert_eq!(original_blocks.len() + 3, regenesis_blocks.len());
    assert_eq!(original_blocks[0], regenesis_blocks[0]);
    assert_eq!(original_blocks[1], regenesis_blocks[1]);

    Ok(())
}
