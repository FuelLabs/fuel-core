use std::str::FromStr;

use chrono::Utc;
use fuel_relayer::{Config, Service};

use ethers_core::types::H160;
use fuel_core_interfaces::{
    block_importer::NewBlockEvent,
    db::helpers::DummyDb,
    model::{BlockHeight, FuelBlock, FuelBlockConsensus, FuelBlockHeader, SealedFuelBlock},
};
use std::sync::Arc;
use tokio::sync::broadcast;

use std::io;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let config = Config {
        da_finalization: 10,
        eth_client: "http://127.0.0.1:8545/".into(),
        eth_chain_id: 1,
        eth_v2_block_commit_contract: H160::from_str("0x2CfFBF63dB3fF10d614B5c93Eedd6E804D5b26ef")
            .unwrap(),
        // this is wETH ERC20 contract.
        eth_v2_contract_addresses: vec![
            H160::from_str("0x2CfFBF63dB3fF10d614B5c93Eedd6E804D5b26ef").unwrap(), // fuel
            H160::from_str("0x7a2F694cE981e354036189D25FEfCC53F43354d4").unwrap(), // validatorSet
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

    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    // let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block1)));
    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    // let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block2)));
    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    // let _ = broadcast_tx.send(NewBlockEvent::Created(Arc::new(block3)));

    Ok(())
}
