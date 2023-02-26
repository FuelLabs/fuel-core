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

    let secret_key: SecretKey = rng.gen();
    let pk = secret_key.public_key();

    let msg1 = MessageConfig {
        sender: rng.gen(),
        recipient: Input::owner(&pk),
        nonce: rng.gen(),
        amount: rng.gen(),
        data: vec![rng.gen()],
        da_height: DaBlockHeight(0),
    };
    let tx1 = TransactionBuilder::script(vec![op::ret(0)].into_iter().collect(), vec![])
        .gas_limit(100000)
        .add_unsigned_message_input(
            secret_key,
            msg1.sender,
            msg1.nonce,
            msg1.amount,
            msg1.data.clone(),
        )
        .finalize_as_transaction();

    let mut node_config = Config::local_node();
    node_config.chain_conf.initial_state = Some(StateConfig {
        messages: Some(vec![msg1]),
        ..Default::default()
    });
    node_config.utxo_validation = true;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // verify tx is successful
    let status = client.submit_and_await_commit(&tx1).await.unwrap();
    assert!(
        matches!(status, TransactionStatus::Success { .. }),
        "expected success, received {status:?}",
    )
}
