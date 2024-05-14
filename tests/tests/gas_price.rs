#![allow(non_snake_case)]

use fuel_core::{
    chain_config::{
        CoinConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    schema::gas_price::EstimateGasPrice,
    types::{
        gas_price::LatestGasPrice,
        primitives::{
            Address,
            AssetId,
        },
    },
    FuelClient,
};
use fuel_core_types::{
    fuel_crypto::{
        coins_bip32::ecdsa::signature::rand_core::SeedableRng,
        SecretKey,
    },
    fuel_tx::{
        Finalizable,
        Input,
        TransactionBuilder,
        UtxoId,
    },
};
use rand::prelude::StdRng;

async fn setup_service_with_coin(
    owner: Address,
    amount: u64,
    static_gas_price: u64,
) -> (FuelService, UtxoId) {
    // setup config
    let tx_id = [0u8; 32];
    let output_index = 0;
    let coin_config = CoinConfig {
        tx_id: tx_id.into(),
        output_index,
        tx_pointer_block_height: Default::default(),
        tx_pointer_tx_idx: 0,
        owner,
        amount,
        asset_id: AssetId::BASE,
    };
    let state = StateConfig {
        coins: vec![coin_config],
        ..Default::default()
    };
    let config = Config {
        static_gas_price,
        ..Config::local_node_with_state_config(state)
    };

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();

    let utxo_id = UtxoId::new(tx_id.into(), output_index);

    (srv, utxo_id)
}

#[tokio::test]
async fn latest_gas_price__should_be_static() {
    // setup node with a block that has a non-mint transaction
    let static_gas_price = 2;
    let max_fee_limit = 100;
    let mut rng = StdRng::seed_from_u64(1234);
    let secret_key: SecretKey = SecretKey::random(&mut rng);
    let pk = secret_key.public_key();
    let owner = Input::owner(&pk);

    let (srv, utxo_id) =
        setup_service_with_coin(owner, max_fee_limit, static_gas_price).await;

    let client = FuelClient::from(srv.bound_address);

    let tx = TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(1)
        .add_unsigned_coin_input(
            secret_key,
            utxo_id,
            max_fee_limit,
            AssetId::BASE,
            Default::default(),
        )
        .finalize()
        .into();

    client.submit_and_await_commit(&tx).await.unwrap();

    // given
    let expected = static_gas_price;

    // when
    let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();

    // then
    let actual = gas_price;
    assert_eq!(expected, actual)
}

#[tokio::test]
async fn latest_gas_price__if_no_mint_tx_in_previous_block_gas_price_is_zero() {
    // given
    let mut node_config = Config::local_node();
    node_config.static_gas_price = 100;
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let LatestGasPrice { gas_price, .. } = client.latest_gas_price().await.unwrap();

    // then
    let expected = 0;
    let actual = gas_price;
    assert_eq!(expected, actual)
}

#[tokio::test]
async fn estimate_gas_price__should_be_static() {
    // given
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let arbitrary_horizon = 10;

    let EstimateGasPrice { gas_price } =
        client.estimate_gas_price(arbitrary_horizon).await.unwrap();

    // then
    let expected = node_config.static_gas_price;
    let actual = u64::from(gas_price);
    assert_eq!(expected, actual);
}
