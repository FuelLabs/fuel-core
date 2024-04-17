use std::net::Ipv4Addr;

use clap::Parser;
use fuel_core_bin::cli::{
    run,
    snapshot,
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};
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
use tokio::task::{
    self,
    JoinHandle,
};

pub struct FuelCoreDriver {
    pub port: u16,
    /// This must be before the db_dir as the drop order matters here
    pub process: JoinHandle<anyhow::Result<()>>,
    pub db_dir: TempDir,
    pub client: FuelClient,
}
impl FuelCoreDriver {
    pub async fn spawn(extra_args: &[&str]) -> Self {
        // Generate temp params
        let db_dir = tempdir().expect("Failed to create temp dir");
        let port = portpicker::pick_unused_port().expect("Failed to pick unused port");

        let port_str = port.to_string();
        let mut args = vec![
            "_IGNORED_",
            "--port",
            &port_str,
            "--db-path",
            db_dir.path().to_str().unwrap(),
        ];
        args.extend(extra_args);

        let process = task::spawn(run::exec(run::Command::parse_from(args)));

        // Wait for the process to open API port
        let mut health_ok = false;
        for _ in 0..100 {
            if let Ok(health) =
                reqwest::get(format!("http://localhost:{port}/health")).await
            {
                assert_eq!(health.status(), 200);
                health_ok = true;
                break;
            } else {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
        if !health_ok {
            panic!("Node failed to open http port");
        }

        Self {
            port,
            process,
            db_dir,
            client: FuelClient::from((Ipv4Addr::LOCALHOST, port)),
        }
    }

    /// Stops the node, returning the db only
    /// Ignoring the return value drops the db as well.
    pub async fn kill(self) -> TempDir {
        self.process.abort();
        self.db_dir
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_old_blocks_are_preserved() -> anyhow::Result<()> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1234);
    let snapshot_dir = tempdir().expect("Failed to create temp dir");

    let core = FuelCoreDriver::spawn(&["--debug", "--poa-instant", "true"]).await;

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

    // Stop the node, keep the db
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    let db_dir = core.kill().await;

    // Take snapshot
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

    // Drop the old db, and create an empty to restore into
    drop(db_dir);

    // Start a new node with the snapshot
    let core = FuelCoreDriver::spawn(&[
        "--debug",
        "--poa-instant",
        "true",
        "--snapshot",
        snapshot_dir.path().to_str().unwrap(),
    ])
    .await;

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
    assert_eq!(original_blocks.len() + 1, regenesis_blocks.len());
    assert_eq!(original_blocks[0], regenesis_blocks[0]);
    assert_eq!(original_blocks[1], regenesis_blocks[1]);

    Ok(())
}
