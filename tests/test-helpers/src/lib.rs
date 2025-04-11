use crate::assemble_tx::SigningAccount;
use fuel_core::{
    chain_config::TESTNET_WALLET_SECRETS,
    service::Config,
};
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_types::{
    fuel_asm::{
        RegId,
        op,
    },
    fuel_crypto::SecretKey,
    fuel_tx::{
        AssetId,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        Upload,
        UploadSubsection,
        policies::Policies,
    },
};
use rand::{
    CryptoRng,
    Rng,
    RngCore,
    rngs::StdRng,
};

pub mod assemble_tx;
pub mod builder;
pub mod counter_contract;
pub mod fuel_core_driver;
pub mod mint_contract;

pub fn predicate() -> Vec<u8> {
    vec![op::ret(1)].into_iter().collect::<Vec<u8>>()
}

pub fn valid_input(rng: &mut StdRng, amount: u64) -> Input {
    let owner = Input::predicate_owner(predicate());
    Input::coin_predicate(
        rng.r#gen(),
        owner,
        amount,
        AssetId::BASE,
        Default::default(),
        Default::default(),
        predicate(),
        vec![],
    )
}

pub fn transactions_from_subsections(
    rng: &mut StdRng,
    subsections: Vec<UploadSubsection>,
    amount: u64,
) -> Vec<Upload> {
    subsections
        .into_iter()
        .map(|subsection| {
            Transaction::upload_from_subsection(
                subsection,
                Policies::new().with_max_fee(amount),
                vec![valid_input(rng, amount)],
                vec![],
                vec![],
            )
        })
        .collect()
}

pub async fn send_graph_ql_query(url: &str, query: &str) -> String {
    let client = reqwest::Client::new();
    let mut map = std::collections::HashMap::new();
    map.insert("query", query);
    let response = client.post(url).json(&map).send().await.unwrap();

    response.text().await.unwrap()
}

pub fn make_tx(
    rng: &mut (impl CryptoRng + RngCore),
    i: u64,
    max_gas_limit: u64,
) -> Transaction {
    TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .script_gas_limit(max_gas_limit / 2)
    .add_unsigned_coin_input(
        SecretKey::random(rng),
        rng.r#gen(),
        1000 + i,
        Default::default(),
        Default::default(),
    )
    .add_output(Output::Change {
        amount: 0,
        asset_id: Default::default(),
        to: rng.r#gen(),
    })
    .finalize_as_transaction()
}

pub async fn produce_block_with_tx(rng: &mut StdRng, client: &FuelClient) {
    let secret = SecretKey::random(rng);
    let script_tx = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_coin_input(
            secret,
            rng.r#gen(),
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

/// Returns a config that enables utxo validation and sets a minimum gas price.
/// It enables fee in the network and requirement to use real coins.
pub fn config_with_fee() -> Config {
    let mut config = Config::local_node();
    config.utxo_validation = true;
    config.txpool.utxo_validation = true;
    config.gas_price_config.min_exec_gas_price = 1000;
    config
}

pub fn default_signing_wallet() -> SigningAccount {
    let wallet_secret: SecretKey = TESTNET_WALLET_SECRETS[1]
        .parse()
        .expect("Expected valid secret");

    SigningAccount::Wallet(wallet_secret)
}
