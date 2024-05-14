use super::*;
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
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
    let tx1 = TransactionBuilder::script(vec![op::ret(0)].into_iter().collect(), vec![])
        .script_gas_limit(100000)
        .add_unsigned_message_input(
            secret_key,
            msg1.sender,
            msg1.nonce,
            msg1.amount,
            msg1.data.clone(),
        )
        .finalize_as_transaction();

    let state = StateConfig {
        messages: vec![msg1],
        ..Default::default()
    };
    let node_config = Config {
        utxo_validation: true,
        ..Config::local_node_with_state_config(state)
    };

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // verify tx is successful
    let status = client.submit_and_await_commit(&tx1).await.unwrap();
    assert!(
        matches!(status, TransactionStatus::Success { .. }),
        "expected success, received {status:?}",
    )
}
