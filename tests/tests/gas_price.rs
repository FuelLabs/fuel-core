#![allow(non_snake_case)]

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};

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
    fuel_asm::*,
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
    services::executor::TransactionExecutionResult,
};
use rand::prelude::StdRng;

async fn setup_service_with_coin(owner: Address, amount: u64) -> (FuelService, UtxoId) {
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

    let (srv, utxo_id) = setup_service_with_coin(owner, max_fee_limit).await;

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
    let node_config = Config::local_node();
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
async fn estimate_gas_price__should_be_estimated_correctly() {
    // given
    let mut node_config = Config::local_node();
    let starting_gas_price = 1000;
    let percent = 10;
    node_config.starting_gas_price = starting_gas_price;
    node_config.gas_price_change_percent = percent;

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // when
    let arbitrary_horizon = 10;

    let EstimateGasPrice { gas_price } =
        client.estimate_gas_price(arbitrary_horizon).await.unwrap();

    // then
    let mut expected = starting_gas_price;
    for _ in 0..arbitrary_horizon {
        expected = expected + (expected * percent / 100);
    }

    let actual = u64::from(gas_price);
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn dry_run_opt_with_zero_gas_price() {
    let tx = TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .add_random_fee_input()
    .script_gas_limit(1000)
    .max_fee_limit(600000)
    .finalize_as_transaction();

    let mut test_builder = TestSetupBuilder::new(2322u64);
    test_builder.min_gas_price = 1;
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let TransactionExecutionResult::Success {
        total_fee,
        total_gas,
        ..
    } = client
        .dry_run_opt(&[tx.clone()], Some(false), None)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .result
    else {
        panic!("dry run should have succeeded");
    };

    let TransactionExecutionResult::Success {
        total_fee: total_fee_zero_gas_price,
        total_gas: total_gas_zero_gas_price,
        ..
    } = client
        .dry_run_opt(&[tx], Some(false), Some(0))
        .await
        .unwrap()
        .pop()
        .unwrap()
        .result
    else {
        panic!("dry run should have succeeded");
    };

    assert_ne!(total_fee, total_fee_zero_gas_price);
    assert_eq!(total_fee_zero_gas_price, 0);

    assert_eq!(total_gas, total_gas_zero_gas_price);
}
