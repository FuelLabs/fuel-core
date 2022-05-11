use std::str::FromStr;

use chrono::{TimeZone, Utc};
use fuel_relayer::{Config, Relayer, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    db::helpers::DummyDb,
    model::{
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
        eth_v2_block_commit_contract: H160::from_str("0x019b0d94d72023B0d0B594797045a77014E3D473")
            .unwrap(),
        // this is wETH ERC20 contract.
        eth_v2_contract_addresses: vec![
            //H160::from_str("0x717833d2641218aAE81a7cB0A2E92E629a2aec0b").unwrap(), //merkle tree
            //H160::from_str("0x9fb707017Ce7fc0AdE7a7c9FEFB247C4531Ae54a").unwrap(), //binaryMerkleTreeLib
            //H160::from_str("0xa7923c6206c43743eE235A9260A96545a8285c86").unwrap(), // merkleSumTreeLib
            H160::from_str("0x019b0d94d72023B0d0B594797045a77014E3D473").unwrap(), // fuel
            //H160::from_str("0x0A3A435C7762b988e8e28606DE04576d0e345DfD").unwrap(), // token
            //H160::from_str("0x0a9EA8C31E48Dc2Af4A7eB211ecDACC4d5865437").unwrap(), // fuelToken
            //H160::from_str("0x4822F3071088668cd98ae64d5B1CC3A3eBfA2344").unwrap(), // guard
            //H160::from_str("0x38426b72293b3946b0BCe1E5653f60B126C49072").unwrap(), // burnAction
            //H160::from_str("0x5c9EB4773e6C5f7E0058bF6976325E09f16b9488").unwrap(), // leaderSelection
            H160::from_str("0xEab18554280f7614e38B167c92bC421d7813F388").unwrap(), // validatorSet
            //H160::from_str("0x85604e4A5151B6b22Ed9AdAAD39b33cfd52504b6").unwrap(), // challengeManager
            //H160::from_str("0x20c2C873408018cf631EEe44893C57f623c25e50").unwrap(), // transactionSerializationLib
            //H160::from_str("0x7671DA4F13f2E116dCe3cDF7498349089BF2Dceb").unwrap(), // signed
        ],
/*
Creating Typechain artifacts in directory typechain for target ethers-v5
Successfully generated Typechain artifacts!
sparseMerkleTreeLib: 0xD8Aa9E7E3B2667efd041E9D80175f06a884E3758
binaryMerkleTreeLib: 0x046630446301f4ED74aE8e38BEb3654F5e7ba32a
merkleSumTreeLib: 0x80016b3AA39C3f9673613997836A8d2fE9920749
fuel: 0x019b0d94d72023B0d0B594797045a77014E3D473
token: 0x68cb955b5CD64554ff46D0b2Ee6b59aC3011134d
fuelToken: 0x9232aF2b21C8c909565c0dE02aA171242Acf6A46
guard: 0x766B807B1f03371f393A987baF4a9bee342D6161
burnAuction: 0xF2f961C3178f34e612aCe104d17cb005765F829f
leaderSelection: 0xC396611631c303BB90507aDa714d09B71Ff602CA
validatorSet: 0xEab18554280f7614e38B167c92bC421d7813F388
challengeManager: 0x913f8580c377c461FC05936ea456aB41Aac7cEC6
transactionSerializationLib: 0x26878b5E93A0f2146437E6Ee704eF9e6380c2B23
 */
        eth_v2_contract_deployment: 0,
        initial_sync_step: 100,
        eth_initial_sync_refresh: std::time::Duration::from_secs(10),
    };

    let mut block = SealedFuelBlock {
        block: FuelBlock {
            header: FuelBlockHeader {
                height: BlockHeight::from(0u64),
                number: BlockHeight::from(10u64),
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

    let db = Box::new(DummyDb::filled());
    db.insert_sealed_block(Arc::new(block.clone()));
    block.block.header.height = BlockHeight::from(1u64);
    db.insert_sealed_block(Arc::new(block.clone()));

    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let signer = Box::new(DummySigner {});

    let provider = Relayer::provider_http(config.eth_client())?;
    let _service = Service::new(&config, db, broadcast_rx, Arc::new(provider)).await?;



    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let _ = broadcast_tx.send(NewBlockEvent::NewBlockCreated(Arc::new(block)));

    tokio::time::sleep(std::time::Duration::from_secs(1000)).await;
    Ok(())
}
