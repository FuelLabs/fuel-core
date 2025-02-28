use fuel_core::{
    chain_config::{
        MessageConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_asm::op,
    fuel_crypto::SecretKey,
    fuel_tx::Input,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use test_helpers::assemble_tx::{
    AssembleAndRunTx,
    SigningAccount,
};

#[tokio::test]
async fn can_submit_genesis_message() {
    let mut rng = StdRng::seed_from_u64(1234);

    let secret_key: SecretKey = SecretKey::random(&mut rng);
    let pk = secret_key.public_key();

    let msg1 = MessageConfig {
        sender: rng.gen(),
        recipient: Input::owner(&pk),
        nonce: rng.gen(),
        amount: rng.gen(),
        data: vec![],
        da_height: DaBlockHeight(0),
    };

    let state = StateConfig {
        messages: vec![msg1],
        ..Default::default()
    };
    let mut node_config = Config {
        utxo_validation: true,
        ..Config::local_node_with_state_config(state)
    };
    node_config.gas_price_config.min_exec_gas_price = 1000;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // verify tx is successful
    let status = client
        .run_script(vec![op::ret(0)], vec![], SigningAccount::Wallet(secret_key))
        .await
        .unwrap();
    assert!(
        matches!(status, TransactionStatus::Success { .. }),
        "expected success, received {status:?}",
    )
}
