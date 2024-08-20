use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::{
        Output,
        TransactionBuilder,
    },
};
use rand::{
    prelude::StdRng,
    Rng,
};

pub mod builder;
pub mod fuel_core_driver;

pub async fn send_graph_ql_query(url: &str, query: &str) -> String {
    let client = reqwest::Client::new();
    let mut map = std::collections::HashMap::new();
    map.insert("query", query);
    let response = client.post(url).json(&map).send().await.unwrap();

    response.text().await.unwrap()
}

pub async fn produce_block_with_tx(rng: &mut StdRng, client: &FuelClient) {
    let secret = SecretKey::random(rng);
    let script_tx = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_coin_input(
            secret,
            rng.gen(),
            1234,
            Default::default(),
            Default::default(),
        )
        .add_output(Output::change(
            Default::default(),
            Default::default(),
            Default::default(),
        ))
        .finalize_as_transaction();
    let status = client
        .submit_and_await_commit(&script_tx)
        .await
        .expect("Failed to send tx");
    assert!(
        matches!(status, TransactionStatus::Success { .. }),
        "{status:?}"
    );
}
