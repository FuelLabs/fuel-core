use std::str::FromStr;

use fuel_core::chain_config::random_testnet_wallet;
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        Address,
        Output,
        Transaction,
        TransactionBuilder,
        Word,
    },
};
use rand::{
    rngs::StdRng,
    CryptoRng,
    Rng,
    RngCore,
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

pub fn make_tx(rng: &mut (impl CryptoRng + RngCore), max_gas_limit: u64) -> Transaction {
    const SMALL_AMOUNT: Word = 2;

    let (wallet, wallet_str) = random_testnet_wallet(rng);

    TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .script_gas_limit(max_gas_limit / 2)
    .add_unsigned_coin_input(
        wallet,
        rng.gen(),
        SMALL_AMOUNT,
        Default::default(),
        Default::default(),
    )
    .add_output(Output::Change {
        amount: SMALL_AMOUNT,
        asset_id: Default::default(),
        to: Address::from_str(wallet_str).expect("should parse bytes as address"),
    })
    .finalize_as_transaction()
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
