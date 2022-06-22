use crate::helpers::create_contract;
use escargot::CargoBuild;
use fuel_core::chain_config::StateConfig;
use fuel_core::database::Database;
use fuel_core::{
    chain_config::{CoinConfig, ContractConfig},
    service::{Config, FuelService},
};
use fuel_core_interfaces::common::{
    fuel_types::{Address, Bytes32, Salt},
    fuel_vm::{
        prelude::{AssetId, Contract, ContractId, InterpreterStorage, Storage},
        util::test_helpers::TestBuilder as TxBuilder,
    },
};
use fuel_gql_client::client::FuelClient;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::time::Duration;
use tempdir::TempDir;

use async_std::task;

#[tokio::test]
async fn snapshot_state_config() {
    let mut db = Database::default();

    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);

    // Extract later for a test case
    let contract = Contract::default();
    let id = ContractId::new([12; 32]);

    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: Some(vec![ContractConfig {
            code: vec![8; 32],
            salt: Salt::new([9; 32]),
            state: None,
            balances: None,
        }]),
        coins: Some(
            vec![
                (owner, 50, asset_id),
                (owner, 100, asset_id),
                (owner, 150, asset_id),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                tx_id: None,
                output_index: None,
                block_created: None,
                maturity: None,
                owner,
                amount,
                asset_id,
            })
            .collect(),
        ),
    });

    Storage::<ContractId, Contract>::insert(&mut db, &id, &contract).unwrap();

    InterpreterStorage::storage_contract_root_insert(
        &mut db,
        &id,
        &Salt::new([5; 32]),
        &Bytes32::new([0; 32]),
    )
    .unwrap();

    // setup server & client
    let _ = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let state_conf = StateConfig::generate_state_config(db);

    assert!(state_conf.contracts.is_some());
    assert!(state_conf.coins.is_some());
}

#[tokio::test]
async fn snapshot_command() {
    let port = portpicker::pick_unused_port().expect("No ports free");

    let tmp_dir = TempDir::new("test").unwrap();
    let tmp_path = tmp_dir.path().to_owned();

    let mut run_cmd = CargoBuild::new()
        .bin("fuel-core")
        .manifest_path("../fuel-core/Cargo.toml")
        .current_release()
        .current_target()
        .run()
        .unwrap()
        .command();

    run_cmd
        .arg("--db-type")
        .arg("rocks-db")
        .arg("--db-path")
        .arg(tmp_path.clone())
        .arg("--port")
        .arg(port.to_string())
        .spawn()
        .unwrap();

    task::sleep(Duration::from_secs(5)).await;

    let client = FuelClient::new(format!("http://127.0.0.1:{}", port.to_string())).unwrap();
    let tx = TxBuilder::new(2322u64)
        .gas_limit(1)
        .coin_input(Default::default(), 1000)
        .change_output(Default::default())
        .build();

    client.submit(&tx).await.unwrap();

    let mut rng = StdRng::seed_from_u64(2322);

    // create a contract in block 1
    // verify a block 2 containing contract id from block 1, with wrong input contract utxo_id
    let (tx2, _) = create_contract(vec![], &mut rng);

    client.submit(&tx2).await.unwrap();

    dbg!(&run_cmd);

    let mut snapshot_cmd = CargoBuild::new()
        .bin("fuel-core")
        .manifest_path("../fuel-core/Cargo.toml")
        .current_release()
        .current_target()
        .run()
        .unwrap()
        .command();

    snapshot_cmd
        .arg("snapshot")
        .arg("--db-path")
        .arg(tmp_path.clone())
        .spawn()
        .unwrap();

    dbg!(&snapshot_cmd);
    //println!("{:?}", snap_result.unwrap().next());
}
