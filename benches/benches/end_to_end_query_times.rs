use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_storage::{
    tables::FuelBlocks,
    transactional::WriteTransaction,
    StorageAsMut,
};

use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_types::BlockHeight,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::local_node();
    let mut db: Database = Database::default();
    let node = FuelService::from_database(db.clone(), config).await?;

    let height = BlockHeight::from(1);
    let block = CompressedBlock::default();

    let mut transaction = db.write_transaction();
    transaction
        .storage::<FuelBlocks>()
        .insert(&height, &block)
        .unwrap();
    // transaction
    //    .storage::<SealedBlockConsensus>()
    //    .insert(&height, &Consensus::PoA(Default::default()))
    //    .unwrap();
    transaction.commit().unwrap();

    let url = format!("http://{}/v1/graphql", node.bound_address);
    let response = test_helpers::send_graph_ql_query(&url, BLOCK_QUERY).await;

    println!("Resp: {response}");

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
