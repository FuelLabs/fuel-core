use std::str::FromStr;

use chrono::Utc;
use fuel_relayer::{Config, Relayer, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    db::helpers::DummyDb,
    model::{BlockHeight, FuelBlock, FuelBlockConsensus, FuelBlockHeader, SealedFuelBlock},
};
use std::sync::Arc;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let config = Config {
        eth_finality_period: 10,
        eth_client: "http://127.0.0.1:8545/".into(),
        eth_chain_id: 1,
        eth_v2_block_commit_contract: H160::from_str("0x2CfFBF63dB3fF10d614B5c93Eedd6E804D5b26ef")
            .unwrap(),
        // this is wETH ERC20 contract.
        eth_v2_contract_addresses: vec![
            //H160::from_str("0x717833d2641218aAE81a7cB0A2E92E629a2aec0b").unwrap(), //merkle tree
            //H160::from_str("0x9fb707017Ce7fc0AdE7a7c9FEFB247C4531Ae54a").unwrap(), //binaryMerkleTreeLib
            //H160::from_str("0xa7923c6206c43743eE235A9260A96545a8285c86").unwrap(), // merkleSumTreeLib
            H160::from_str("0x2CfFBF63dB3fF10d614B5c93Eedd6E804D5b26ef").unwrap(), // fuel
            //H160::from_str("0x0A3A435C7762b988e8e28606DE04576d0e345DfD").unwrap(), // token
            //H160::from_str("0x0a9EA8C31E48Dc2Af4A7eB211ecDACC4d5865437").unwrap(), // fuelToken
            //H160::from_str("0x4822F3071088668cd98ae64d5B1CC3A3eBfA2344").unwrap(), // guard
            //H160::from_str("0x38426b72293b3946b0BCe1E5653f60B126C49072").unwrap(), // burnAction
            //H160::from_str("0x5c9EB4773e6C5f7E0058bF6976325E09f16b9488").unwrap(), // leaderSelection
            H160::from_str("0x7a2F694cE981e354036189D25FEfCC53F43354d4").unwrap(), // validatorSet
                                                                                   //H160::from_str("0x85604e4A5151B6b22Ed9AdAAD39b33cfd52504b6").unwrap(), // challengeManager
                                                                                   //H160::from_str("0x20c2C873408018cf631EEe44893C57f623c25e50").unwrap(), // transactionSerializationLib
                                                                                   //H160::from_str("0x7671DA4F13f2E116dCe3cDF7498349089BF2Dceb").unwrap(), // signed
        ],
        /*
        Creating Typechain artifacts in directory typechain for target ethers-v5
        Successfully generated Typechain artifacts!
        sparseMerkleTreeLib: 0x93aA8245018d568a477A0a92526ca79D18e5eA96
        binaryMerkleTreeLib: 0xCD9A625c19788E630F0A70CD8243bD8e020bb9a6
        merkleSumTreeLib: 0xc599Bba784bDADbC9096A6c65Cd88f8124c91d7c
        fuel: 0x65080A1442A6F02B99923e004832709827B56D08
        token: 0x7B9ae63edf186356781Efcd804cb0a1C17761098
        fuelToken: 0xedd5BF6F8a6F9f312331F4F11C0A4Ff2512FB810
        guard: 0x21e337432160c2a2756FDbC1e86f26b4C3EF79c7
        burnAuction: 0xDa59722Bae2d8FAe4DcB804327c661d2696C2516
        leaderSelection: 0xe0C8C6C43e299783E59A26E6f1e56E5B1acc6cA7
        validatorSet: 0x57BCFa6e06947cbDc4c0A8D627D2e8f44F0277d8
        challengeManager: 0x027e900Ce0543431E7691795Ac2fA37ed2151a2b
        transactionSerializationLib: 0x488490184b37A26E493c6de2c2792585cbB82862
        Initial token amount:  1000000.0
                 */
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
