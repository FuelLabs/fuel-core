use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::local_node();
    let db: Database = Database::default();
    let node = FuelService::from_database(db.clone(), config).await?;

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
