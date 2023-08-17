use fuel_core::{
    chain_config::{CoinConfig, StateConfig},
    service::{Config, FuelService},
use fuel_core::p2p_test_helpers::{
    make_nodes,
    BootstrapSetup,
    Nodes,
    ProducerSetup,
    ValidatorSetup,
};
use fuel_core_client::client::FuelClient;
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_tx::{
        field::*,
        input::coin::{CoinPredicate, CoinSigned},
        *,
    },
    fuel_vm::*,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::time::Duration;

fn create_node_config_from_inputs(inputs: &[Input]) -> Config {
    let mut node_config = Config::local_node();
    let mut initial_state = StateConfig::default();
    let mut coin_configs = vec![];

    for input in inputs {
        if let Input::CoinSigned(CoinSigned {
            amount,
            owner,
            asset_id,
            utxo_id,
            ..
        })
        | Input::CoinPredicate(CoinPredicate {
            amount,
            owner,
            asset_id,
            utxo_id,
            ..
        }) = input
        {
            let coin_config = CoinConfig {
                tx_id: Some(*utxo_id.tx_id()),
                output_index: Some(utxo_id.output_index()),
                tx_pointer_block_height: None,
                tx_pointer_tx_idx: None,
                maturity: None,
                owner: *owner,
                amount: *amount,
                asset_id: *asset_id,
            };
            coin_configs.push(coin_config);
        };
    }

    initial_state.coins = Some(coin_configs);
    node_config.chain_conf.initial_state = Some(initial_state);
    node_config.utxo_validation = true;
    node_config.p2p.as_mut().unwrap().enable_mdns = true;
    node_config
}
    fuel_tx::*,
    fuel_vm::*,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{
        Hash,
        Hasher,
    },
    time::Duration,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossiping() {
    use futures::StreamExt;
    // Create a random seed based on the test parameters.
    let mut hasher = DefaultHasher::new();
    let num_txs = 1;
    let num_validators = 1;
    let num_partitions = 1;
    (num_txs, num_validators, num_partitions, line!()).hash(&mut hasher);
    let mut rng = StdRng::seed_from_u64(hasher.finish());

    // Create a set of key pairs.
    let secrets: Vec<_> = (0..1).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys: Vec<_> = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()))
        .collect();

    // Create a producer for each key pair and a set of validators that share
    // the same key pair.
    let Nodes {
        producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        pub_keys
            .iter()
            .map(|pub_key| Some(BootstrapSetup::new(*pub_key))),
        secrets.clone().into_iter().enumerate().map(|(i, secret)| {
            Some(
                ProducerSetup::new(secret)
                    .with_txs(num_txs)
                    .with_name(format!("{}:producer", pub_keys[i])),
            )
        }),
        pub_keys.iter().flat_map(|pub_key| {
            (0..num_validators).map(move |i| {
                Some(ValidatorSetup::new(*pub_key).with_name(format!("{pub_key}:{i}")))
            })
        }),
    )
    .await;

    let client_one = FuelClient::from(producers[0].node.bound_address);
    let client_two = FuelClient::from(validators[0].node.bound_address);

    let (tx_id, _) = producers[0]
        .insert_txs()
        .await
        .into_iter()
        .next()
        .expect("Producer is initialized with one transaction");

    let _ = client_one.await_transaction_commit(&tx_id).await.unwrap();

    let mut client_two_subscription = client_two
        .subscribe_transaction_status(&tx_id)
        .await
        .expect("Should be able to subscribe for events");
    tokio::time::timeout(Duration::from_secs(10), client_two_subscription.next())
        .await
        .expect("Should await transaction notification in time");

    let response = client_two.transaction(&tx_id).await.unwrap();
    assert!(response.is_some());
}
