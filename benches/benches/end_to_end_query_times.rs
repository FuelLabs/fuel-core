use fuel_core::{
    database::Database,
    service::{
        config::Trigger,
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_storage::{
    tables::FuelBlocks,
    transactional::WriteTransaction,
    StorageAsMut,
};

use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_types::BlockHeight,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config {
        block_production: Trigger::Never,
        ..Config::local_node()
    };
    // let mut db: Database = Database::default();
    // let node = FuelService::from_database(db.clone(), config).await?;

    let node = FuelService::new_node(config).await?;

    let height = BlockHeight::from(1);
    let block = CompressedBlock::default();
    let client = FuelClient::from(node.bound_address);

    // let mut transaction = db.write_transaction();
    // transaction
    //     .storage::<FuelBlocks>()
    //     .insert(&height, &block)
    //     .unwrap();
    // // transaction
    // //    .storage::<SealedBlockConsensus>()
    // //    .insert(&height, &Consensus::PoA(Default::default()))
    // //    .unwrap();
    // transaction.commit().unwrap();

    let tx_count: u64 = 66_000;
    let max_gas_limit = 50_000_000;
    let mut rng = StdRng::seed_from_u64(2322);
    for tx in (1..=tx_count).map(|i| test_helpers::make_tx(&mut rng, i, max_gas_limit)) {
        let _tx_id = client.submit(&tx).await?;
    }

    let last_block_height = client.produce_blocks(1, None).await?;

    // let url = format!("http://{}/v1/graphql", node.bound_address);
    // let response = test_helpers::send_graph_ql_query(&url, BLOCK_QUERY).await;
    // println!("Resp: {response}");

    let addr = node.bound_address;
    println!("Serving at: {addr}");
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;

    Ok(())
}

const BLOCK_QUERY: &'static str = r#"
    query {
      block(height: "0") {
        id,
        transactions {
          id
        }
      }
    }
"#;
