use std::str::FromStr;

use chrono::{TimeZone, Utc};
use fuel_relayer::{Config, Relayer, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    db::helpers::DummyDb,
    model::{BlockHeight, FuelBlock, FuelBlockConsensus, FuelBlockHeader, SealedFuelBlock},
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
        eth_v2_block_commit_contract: H160::from_str("0x4Fe74afc07623f082e7e592735e5b81F461A04a5")
            .unwrap(),
        // this is wETH ERC20 contract.
        eth_v2_contract_addresses: vec![
            //H160::from_str("0x717833d2641218aAE81a7cB0A2E92E629a2aec0b").unwrap(), //merkle tree
            //H160::from_str("0x9fb707017Ce7fc0AdE7a7c9FEFB247C4531Ae54a").unwrap(), //binaryMerkleTreeLib
            //H160::from_str("0xa7923c6206c43743eE235A9260A96545a8285c86").unwrap(), // merkleSumTreeLib
            H160::from_str("0x4Fe74afc07623f082e7e592735e5b81F461A04a5").unwrap(), // fuel
            //H160::from_str("0x0A3A435C7762b988e8e28606DE04576d0e345DfD").unwrap(), // token
            //H160::from_str("0x0a9EA8C31E48Dc2Af4A7eB211ecDACC4d5865437").unwrap(), // fuelToken
            //H160::from_str("0x4822F3071088668cd98ae64d5B1CC3A3eBfA2344").unwrap(), // guard
            //H160::from_str("0x38426b72293b3946b0BCe1E5653f60B126C49072").unwrap(), // burnAction
            //H160::from_str("0x5c9EB4773e6C5f7E0058bF6976325E09f16b9488").unwrap(), // leaderSelection
            H160::from_str("0x0886d4825e5e26eafeAACfc30913D380d75b715B").unwrap(), // validatorSet
                                                                                   //H160::from_str("0x85604e4A5151B6b22Ed9AdAAD39b33cfd52504b6").unwrap(), // challengeManager
                                                                                   //H160::from_str("0x20c2C873408018cf631EEe44893C57f623c25e50").unwrap(), // transactionSerializationLib
                                                                                   //H160::from_str("0x7671DA4F13f2E116dCe3cDF7498349089BF2Dceb").unwrap(), // signed
        ],
        /*
        Creating Typechain artifacts in directory typechain for target ethers-v5
        sparseMerkleTreeLib: 0xbB4924c080dF19b45662C9680cE8E6E9ee77e21F
        binaryMerkleTreeLib: 0xD2FDb55eE3710d59d9ED7AF13B479E344CC48C39
        merkleSumTreeLib: 0x5A00870C511ECEf2BeDaD4b31F3029F675B3CDeF
        fuel: 0x8026C45c8B9eFe4143E0bA0935961B8C81ab3588
        token: 0x86da2df0Ce4ead61E15336D8fb7B7323C5d22202
        fuelToken: 0xFDc8b4a3007349ebDB2406bf32e6A3F814c61522
        guard: 0x0768aa59d7fFE416407f9F50eFFB78F70Cc8b42d
        burnAuction: 0xBBeA7A62A3841985100Bbc00D71Bb15191d7760d
        leaderSelection: 0xF7C859E12616a2E2DCA98873877739E25b6D618A
        validatorSet: 0xF106981FC821F66264Bb1D1af6E2d1D87fb16163
        challengeManager: 0x57298092055D84261347447aC553651D0DC8A32d
        transactionSerializationLib: 0x3C79D68150C41f40897A2e63a78aA96b3A5f48cf
        Initial token amount:  1000000.0
         */
        eth_v2_contract_deployment: 0,
        initial_sync_step: 100,
        eth_initial_sync_refresh: std::time::Duration::from_secs(10),
    };

    let block = SealedFuelBlock {
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
    let mut block1 = block.clone();
    block1.block.header.height = BlockHeight::from(1u64);
    db.insert_sealed_block(Arc::new(block1.clone()));
    let mut block2 = block.clone();
    block2.block.header.height = BlockHeight::from(2u64);
    db.insert_sealed_block(Arc::new(block2.clone()));
    let mut block3 = block.clone();
    block3.block.header.height = BlockHeight::from(3u64);
    db.insert_sealed_block(Arc::new(block3.clone()));

    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);

    let provider = Relayer::provider_http(config.eth_client())?;
    let _service = Service::new(&config, db, broadcast_rx, Arc::new(provider)).await?;

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block1)));
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block2)));
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block3)));

    tokio::time::sleep(std::time::Duration::from_secs(1000)).await;
    Ok(())
}
