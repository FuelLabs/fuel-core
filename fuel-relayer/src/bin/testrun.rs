use chrono::Utc;
use ethers_core::types::H160;
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    db::helpers::DummyDb,
    model::{BlockHeight, FuelBlock, FuelBlockConsensus, FuelBlockHeader, SealedFuelBlock},
};
use fuel_relayer::{Config, Service};
use std::{io, str::FromStr, sync::Arc};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let config = Config {
        da_finalization: 10,
        eth_client: "http://127.0.0.1:8545/".into(),
        eth_chain_id: 1,
        eth_v2_block_commit_contract: H160::from_str("0x2CfFBF63dB3fF10d614B5c93Eedd6E804D5b26ef")
            .unwrap(),
        eth_v2_contract_addresses: vec![
            H160::from_str("0x2CfFBF63dB3fF10d614B5c93Eedd6E804D5b26ef").unwrap(), // fuel
            H160::from_str("0x7a2F694cE981e354036189D25FEfCC53F43354d4").unwrap(), // validatorSet
        ],
        eth_v2_contract_deployment: 0,
        initial_sync_step: 100,
        eth_initial_sync_refresh: std::time::Duration::from_secs(10),
    };

    let block = SealedFuelBlock {
        block: FuelBlock {
            header: FuelBlockHeader {
                number: BlockHeight::from(10u64),
                time: Utc::now(),
                ..Default::default()
            },
            transactions: Vec::new(),
        },
        consensus: FuelBlockConsensus {
            required_stake: 10,
            ..Default::default()
        },
    };

    let db = Box::new(DummyDb::filled());
    for i in 0..100 {
        let mut block = block.clone();
        block.block.header.height = BlockHeight::from(i as u64);
        db.insert_sealed_block(Arc::new(block));
    }

    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let provider = Service::provider_http(config.eth_client())?;

    let private_key =
        hex::decode("c6bd905dcac2a0b1c43f574ab6933df14d7ceee0194902bce523ed054e8e798b").unwrap();
    let _service =
        Service::new(&config, &private_key, db, broadcast_rx, Arc::new(provider)).await?;

    let mut next = 1u64;
    loop {
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");
        match input.as_str().chars().next() {
            Some('n') => {
                let mut block = block.clone();
                block.block.header.height = BlockHeight::from(next);
                let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block)));
                next += 1;
            }
            Some('q') => break,
            _ => {}
        }
    }

    Ok(())
}
