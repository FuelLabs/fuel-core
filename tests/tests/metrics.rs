use fuel_core::service::{
    Config,
    DbType,
    FuelService,
    ServiceTrait,
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
};
use tempfile::TempDir;

#[tokio::test]
async fn test_metrics_endpoint() {
    let mut config = Config::local_node();
    let tmp_dir = TempDir::new().unwrap();
    config.database_type = DbType::RocksDb;
    config.database_path = tmp_dir.path().to_path_buf();
    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();

    let client = FuelClient::from(srv.bound_address);
    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);
    // Should generate some database reads
    client.balance(&owner, Some(&asset_id)).await.unwrap();

    let script = [
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    client
        .submit_and_await_commit(
            &TransactionBuilder::script(script, vec![])
                .script_gas_limit(1000000)
                .add_random_fee_input()
                .finalize_as_transaction(),
        )
        .await
        .unwrap();

    let resp = reqwest::get(format!("http://{}/metrics", srv.bound_address))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let categories = resp.split('\n').collect::<Vec<&str>>();

    srv.stop_and_await().await.unwrap();

    // Gt check exists because testing can be weird with multiple instances running
    assert!(categories.len() >= 16);
}
