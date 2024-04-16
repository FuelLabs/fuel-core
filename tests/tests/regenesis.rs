use std::{
    ffi::OsStr,
    net::Ipv4Addr,
    path::PathBuf,
    process::Stdio,
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
use tokio::{
    io::{
        AsyncBufReadExt,
        BufReader,
    },
    process::Command,
    sync::oneshot,
};

fn workspace_root_dir() -> PathBuf {
    let root: PathBuf = env!("CARGO_MANIFEST_DIR").into();
    root.parent().unwrap().to_owned()
}

pub struct FuelCoreDriver {
    pub port: u16,
    /// This must be before the db_dir as the drop order matters here
    pub process: tokio::process::Child,
    pub db_dir: TempDir,
    pub client: FuelClient,
}
impl FuelCoreDriver {
    pub async fn spawn<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        // Generate temp params
        let db_dir = tempdir().expect("Failed to create temp dir");
        let port = portpicker::pick_unused_port().expect("Failed to pick unused port");

        let mut process = Command::new(env!("CARGO"))
            .arg("run")
            .arg("--bin")
            .arg("fuel-core")
            .arg("--features")
            .arg("parquet")
            .arg("--")
            .arg("run")
            .arg("--port")
            .arg(port.to_string())
            .arg("--db-path")
            .arg(db_dir.path())
            .args(args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::piped())
            .current_dir(workspace_root_dir())
            .spawn()
            .expect("Failed to spawn fuel-core process");

        // Wait for the node to start up.
        // Relay all lines to parent processes stderr.
        let (startup_tx, startup_rx) = oneshot::channel();
        let stderr = process.stderr.take().unwrap();
        tokio::spawn(async move {
            let mut startup_tx = Some(startup_tx);
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("{}", line); // Forward to parent stderr for easier debugging
                if line.contains("Starting GraphQL service") {
                    if let Some(c) = startup_tx.take() {
                        let _ = c.send(());
                    }
                }
            }
        });

        startup_rx.await.expect("Failed to start fuel-core process");
        Self {
            port,
            process,
            db_dir,
            client: FuelClient::from((Ipv4Addr::LOCALHOST, port)),
        }
    }

    /// St.ops the node, returning the db only
    /// Ignoring the return value drops the db as well.
    pub async fn kill(mut self) -> TempDir {
        self.process
            .kill()
            .await
            .expect("Failed to kill fuel-core process");
        self.db_dir
    }

    pub async fn healthcheck(&self) {
        let health = reqwest::get(format!("http://localhost:{}/health", self.port))
            .await
            .expect("Failed to get health endpoint");
        assert_eq!(health.status(), 200);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis_old_blocks_are_preserved() -> anyhow::Result<()> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1234);
    let snapshot_dir = tempdir().expect("Failed to create temp dir");

    let core = FuelCoreDriver::spawn(&["--debug", "--poa-instant", "true"]).await;
    core.healthcheck().await;

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
    let tx_status = core
        .client
        .submit_and_await_commit(&contract_tx)
        .await
        .unwrap();

    println!("tx_status: {:?}", tx_status);

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
    let status = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--bin")
        .arg("fuel-core")
        .arg("--features")
        .arg("parquet")
        .arg("--")
        .arg("snapshot")
        .arg("--output-directory")
        .arg(snapshot_dir.path())
        .arg("--db-path")
        .arg(db_dir.path())
        .arg("everything")
        .arg("encoding")
        .arg("parquet")
        .current_dir(workspace_root_dir())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .expect("Failed to run fuel-core snapshot process");
    assert!(status.success());

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
    core.healthcheck().await;

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
