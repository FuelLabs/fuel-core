use std::time::Instant;

use fuel_core::{
    database::Database,
    fuel_core_graphql_api::ServiceConfig,
    service::{
        config::Trigger,
        Config,
        FuelService,
    },
};
use fuel_core_chain_config::Randomize;
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};
use fuel_core_storage::{
    tables::FuelBlocks,
    transactional::WriteTransaction,
    StorageAsMut,
};

use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_tx::Address,
    fuel_types::BlockHeight,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(2322);

    // let graphql_config = ServiceConfig {
    //    max_queries_complexity: usize::MAX,
    //};

    let mut config = Config::local_node();

    config.block_production = Trigger::Never;
    config.graphql_config.max_queries_complexity = usize::MAX;

    // let mut db: Database = Database::default();
    // let node = FuelService::from_database(db.clone(), config).await?;

    let node = FuelService::new_node(config).await?;

    // let height = BlockHeight::from(1);
    // let block = CompressedBlock::default();
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

    let tx_count: u64 = 1_000;
    let max_gas_limit = 50_000_000;

    let elon_musk = Address::randomize(&mut rng);

    for _ in 0..100 {
        for tx in (1..=tx_count).map(|i| {
            test_helpers::make_tx_with_recipient(&mut rng, i, max_gas_limit, elon_musk)
        }) {
            let _tx_id = client.submit(&tx).await?;
        }
        let _last_block_height = client.produce_blocks(10, None).await?;
    }

    let request = PaginationRequest {
        cursor: None,
        results: 100_000,
        direction: PageDirection::Forward,
    };

    let before_query = Instant::now();
    let transaction_response = client.transactions_by_owner(&elon_musk, request).await?;
    let cursor = transaction_response.cursor;
    let has_next_page = transaction_response.has_next_page;
    let query_time = before_query.elapsed().as_millis();

    println!("Elapsed: {query_time}");
    println!("Transactions len: {}", transaction_response.results.len());
    println!("Cursor: {cursor:?}");
    println!("Has next page: {has_next_page}");

    // let url = format!("http://{}/v1/graphql", node.bound_address);
    // let response = test_helpers::send_graph_ql_query(&url, BLOCK_QUERY).await;
    // println!("Resp: {response}");

    // let addr = node.bound_address;
    // println!("Serving at: {addr}");
    // tokio::time::sleep(std::time::Duration::from_secs(3600)).await;

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
