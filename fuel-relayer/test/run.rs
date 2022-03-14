use fuel_relayer::{Config, Relayer, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{db::helpers::DummyDb,
    signer::helpers::DummySigner,
};
use tokio::sync::{broadcast, mpsc, Mutex};

use tracing::info;
use tracing_subscriber;

#[tokio::test]
async fn main_run() -> Result<(),anyhow::Error> {

    tracing_subscriber::fmt::init();

    info!("Hello world");
    let config = Config {
        eth_finality_slider: 10,
        eth_client: "wss://mainnet.infura.io/ws/v3/0954246eab5544e89ac236b668980810".into(),
        eth_v2_contract_addresses: vec![H160::zero()],
        eth_v2_contract_deployment: 15_000_090,
        initial_sync_step: 1000,
        eth_initial_sync_refresh: std::time::Duration::from_secs(10),
    };

    let db = Box::new(Mutex::new(DummyDb::filled()));
    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let signer = Box::new(DummySigner {});
    info!("TEST1");
    let service = Service::new(&config,db,broadcast_rx,signer).await?;
    //let sender = service.sender().clone();

    tokio::time::sleep(std::time::Duration::from_secs(1000)).await;
    Ok(())
}
