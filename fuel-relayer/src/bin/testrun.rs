use std::str::FromStr;

use fuel_relayer::{Config, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{db::helpers::DummyDb, signer::helpers::DummySigner};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    let config = Config {
        eth_finality_period: 10,
        eth_client: "wss://mainnet.infura.io/ws/v3/0954246eab5544e89ac236b668980810".into(),
        // this is wETH ERC20 contract.
        eth_v2_contract_addresses: vec![H160::from_str(
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        )
        .unwrap()],
        eth_v2_contract_deployment: 14_380_000,
        initial_sync_step: 100,
        eth_initial_sync_refresh: std::time::Duration::from_secs(10),
    };

    let db = Box::new(DummyDb::filled());
    let (_broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let signer = Box::new(DummySigner {});
    let _service = Service::new(&config, db, broadcast_rx, signer).await?;

    tokio::time::sleep(std::time::Duration::from_secs(1000)).await;
    Ok(())
}
