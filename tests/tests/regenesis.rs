#![warn(warnings)]
use std::{net::{IpAddr, Ipv4Addr}, path::PathBuf, process::Stdio};

use fuel_core_client::client::{pagination::{PageDirection, PaginationRequest}, FuelClient};
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
    fuel_types::canonical::Serialize,
    fuel_vm::{
        checked_transaction::IntoChecked,
        *,
    },
};
use rand::SeedableRng;
use tempfile::tempdir;
use tokio::{io::{AsyncBufReadExt, BufReader}, process::Command};


#[tokio::test(flavor = "multi_thread")]
async fn test_regenesis() -> anyhow::Result<()> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1234);

    let db_dir = tempdir().expect("Failed to create temp dir");
    let snapshot_dir = tempdir().expect("Failed to create temp dir");
    let port = portpicker::pick_unused_port().expect("Failed to pick unused port");

    // Get workspace root dir
    let root: PathBuf = env!("CARGO_MANIFEST_DIR").into();
    let root = root.parent().unwrap();

    let mut process = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--bin")
        .arg("fuel-core")
        .arg("--")
        .arg("run")
        .arg("--debug")
        .arg("--poa-instant")
        .arg("true")
        .arg("--port")
        .arg(port.to_string())
        .arg("--db-path")
        .arg(db_dir.path())
        .stderr(Stdio::piped())
        .current_dir(root)
        .spawn()
        .expect("Failed to spawn fuel-core process");

    // Wait for the node to start up
    let stderr = process.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr).lines();
    while let Some(line) = reader.next_line().await? {
        println!("{}", line);
        if line.contains("Starting GraphQL service") {
            break;
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    // Ensure it's running
    let health = reqwest::get(format!("http://localhost:{port}/health")).await.expect("Failed to get health endpoint");
    assert_eq!(health.status(), 200);

    // Add some blocks
    let client = FuelClient::from((Ipv4Addr::LOCALHOST, port));

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

        client
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
    let tx_status = client.submit_and_await_commit(&contract_tx).await.unwrap();

    println!("tx_status: {:?}", tx_status);

    let original_blocks = client.blocks(PaginationRequest {
        cursor: None,
        results: 100,
        direction: PageDirection::Forward,
    }).await.expect("Failed to get blocks").results;

    process.kill().await.expect("Failed to kill fuel-core process");

    // Take snapshot
    let status = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--bin")
        .arg("fuel-core")
        .arg("--")
        .arg("snapshot")
        .arg("--output-directory")
        .arg(snapshot_dir.path())
        .arg("--db-path")
        .arg(db_dir.path())
        .arg("everything")
        .current_dir(root)
        .status()
        .await
        .expect("Failed to run fuel-core snapshot process");
    assert!(status.success());

    // Drop the old db, and create an empty to restore into
    drop(db_dir);
    let db_dir = tempdir().expect("Failed to create temp dir");

    // Start a new node with the snapshot
    let mut process = Command::new(env!("CARGO"))
        .arg("run")
        .arg("--bin")
        .arg("fuel-core")
        .arg("--")
        .arg("run")
        .arg("--debug")
        .arg("--poa-instant")
        .arg("true")
        .arg("--port")
        .arg(port.to_string())
        .arg("--db-path")
        .arg(db_dir.path())
        .arg("--snapshot")
        .arg(snapshot_dir.path())
        .stderr(Stdio::piped())
        .current_dir(root)
        .spawn()
        .expect("Failed to spawn fuel-core process");

    // Wait for the node to start up
    let stderr = process.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr).lines();
    while let Some(line) = reader.next_line().await? {
        println!("{}", line);
        if line.contains("Starting GraphQL service") {
            break;
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Ensure it's running
    let health = reqwest::get(format!("http://localhost:{port}/health")).await.expect("Failed to get health endpoint");
    assert_eq!(health.status(), 200);

    let regenesis_blocks = client.blocks(PaginationRequest {
        cursor: None,
        results: 100,
        direction: PageDirection::Forward,
    }).await.expect("Failed to get blocks").results;

    dbg!(original_blocks.len(), regenesis_blocks.len());

    panic!("Ok!?");
    // Ok(())
}