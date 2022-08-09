use fuel_core::{
    config::{
        chain_config::{DaMessageConfig, StateConfig},
        Config,
    },
    service::FuelService,
};
use fuel_core_interfaces::common::fuel_tx::TransactionBuilder;
use fuel_crypto::SecretKey;
use fuel_gql_client::client::FuelClient;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::ops::Deref;

#[tokio::test]
async fn can_submit_genesis_message() {
    let mut rng = StdRng::seed_from_u64(1234);

    let secret_key: SecretKey = rng.gen();
    let owner = secret_key.public_key().hash();

    let msg1 = DaMessageConfig {
        sender: rng.gen(),
        recipient: rng.gen(),
        owner: (*owner.deref()).into(),
        nonce: rng.gen(),
        amount: rng.gen(),
        data: vec![rng.gen()],
        da_height: 0,
    };
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_message_input(
            secret_key,
            msg1.sender,
            msg1.recipient,
            msg1.nonce,
            msg1.amount,
            msg1.data.clone(),
        )
        .finalize();

    let mut node_config = Config::local_node();
    node_config.chain_conf.initial_state = Some(StateConfig {
        messages: Some(vec![msg1]),
        ..Default::default()
    });
    node_config.utxo_validation = true;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    client.submit(&tx1).await.unwrap();
}
