use std::str::FromStr;

use chrono::{TimeZone, Utc};
use fuel_relayer::{Config, Relayer, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    db::helpers::DummyDb,
    primitive_types::{
        BlockHeight, FuelBlock, FuelBlockConsensus, FuelBlockHeader, SealedFuelBlock,
    },
    signer::helpers::DummySigner,
};
use fuel_tx::{Address, Bytes32};
use std::sync::Arc;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let config = Config {
        eth_finality_period: 10,
        eth_client: "http://127.0.0.1:8545/".into(),
        eth_chain_id: 1,
        eth_v2_block_commit_contract: H160::from_str("0xe3c64D24191A2c626e7a4Bc4Ff7371F1BE510d92")
            .unwrap(),
        // this is wETH ERC20 contract.
        eth_v2_contract_addresses: vec![
            //H160::from_str("0x717833d2641218aAE81a7cB0A2E92E629a2aec0b").unwrap(), //merkle tree
            //H160::from_str("0x9fb707017Ce7fc0AdE7a7c9FEFB247C4531Ae54a").unwrap(), //binaryMerkleTreeLib
            //H160::from_str("0xa7923c6206c43743eE235A9260A96545a8285c86").unwrap(), // merkleSumTreeLib
            H160::from_str("0xe3c64D24191A2c626e7a4Bc4Ff7371F1BE510d92").unwrap(), // fuel
            //H160::from_str("0x0A3A435C7762b988e8e28606DE04576d0e345DfD").unwrap(), // token
            //H160::from_str("0x0a9EA8C31E48Dc2Af4A7eB211ecDACC4d5865437").unwrap(), // fuelToken
            //H160::from_str("0x4822F3071088668cd98ae64d5B1CC3A3eBfA2344").unwrap(), // guard
            //H160::from_str("0x38426b72293b3946b0BCe1E5653f60B126C49072").unwrap(), // burnAction
            //H160::from_str("0x5c9EB4773e6C5f7E0058bF6976325E09f16b9488").unwrap(), // leaderSelection
            H160::from_str("0x422E1022369AcC085EeDF5E0Af1Db09363953451").unwrap(), // validatorSet
            //H160::from_str("0x85604e4A5151B6b22Ed9AdAAD39b33cfd52504b6").unwrap(), // challengeManager
            //H160::from_str("0x20c2C873408018cf631EEe44893C57f623c25e50").unwrap(), // transactionSerializationLib
            //H160::from_str("0x7671DA4F13f2E116dCe3cDF7498349089BF2Dceb").unwrap(), // signed
        ],

        eth_v2_contract_deployment: 0,
        initial_sync_step: 100,
        eth_initial_sync_refresh: std::time::Duration::from_secs(10),
    };

    let db = Box::new(DummyDb::filled());
    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let signer = Box::new(DummySigner {});

    let provider = Relayer::provider_http(config.eth_client())?;
    let _service = Service::new(&config, db, broadcast_rx, signer, Arc::new(provider)).await?;

    let block = SealedFuelBlock {
        block: FuelBlock {
            header: FuelBlockHeader {
                height: BlockHeight::from(10u64),
                number: BlockHeight::from(1u64),
                parent_hash: Bytes32::zeroed(),
                prev_root: Bytes32::zeroed(),
                transactions_root: Bytes32::zeroed(),
                time: Utc.ymd(2018, 1, 26).and_hms_micro(18, 30, 9, 453_829),
                producer: Address::zeroed(),
            },
            transactions: Vec::new(),
        },
        consensus: FuelBlockConsensus {
            required_stake: 10,
            validators: Vec::new(),
            stakes: Vec::new(),
            signatures: Vec::new(),
        },
    };
    let _ = broadcast_tx.send(NewBlockEvent::NewBlockCreated(Arc::new(block)));

    tokio::time::sleep(std::time::Duration::from_secs(1000)).await;
    Ok(())
}
