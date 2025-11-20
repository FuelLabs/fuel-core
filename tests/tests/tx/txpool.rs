//! Tests the behaviour of the tx pool.

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::service::FuelService;
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::*,
    fuel_tx,
    fuel_tx::*,
};
use futures::StreamExt;
use itertools::Itertools;
use rand::{
    Rng,
    SeedableRng,
    rngs::StdRng,
};
use std::time::Duration;
use test_helpers::{
    assemble_tx::AssembleAndRunTx,
    config_with_fee,
    default_signing_wallet,
};

#[tokio::test]
async fn txs_max_script_gas_limit() {
    const MAX_GAS_LIMIT: u64 = 5_000_000_000;
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.gas_limit = Some(MAX_GAS_LIMIT);
    // initialize 10 random transactions that transfer coins
    let transactions = (1..=10)
        .map(|i| {
            TransactionBuilder::script(
                op::ret(RegId::ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .script_gas_limit(MAX_GAS_LIMIT / 2)
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
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
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions.iter().collect_vec());
    test_builder.trigger = Trigger::Never;

    // spin up node
    let TestContext { client, srv, .. } = test_builder.finalize().await;

    // submit transactions and verify their status
    let txs = transactions
        .clone()
        .into_iter()
        .map(fuel_tx::Transaction::from)
        .collect::<Vec<_>>();
    for tx in txs {
        srv.shared.txpool_shared_state.insert(tx).await.unwrap();
    }

    client.produce_blocks(1, None).await.unwrap();
    let block = client.block_by_height(1.into()).await.unwrap().unwrap();
    assert_eq!(
        block.transactions.len(),
        transactions.len() + 1 // coinbase
    )
}

#[tokio::test]
async fn informs_immediately_if_the_input_was_spent_during_open_period() {
    let mut config = config_with_fee();
    config.block_production = Trigger::Open {
        period: Duration::from_secs(1_000_000),
    };

    // Given
    config.txpool.pending_pool_tx_ttl = Duration::from_secs(1_000_000);

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let script = vec![op::ret(RegId::ONE)];
    let tx = client
        .assemble_script(script, vec![], default_signing_wallet())
        .await
        .unwrap();

    let status = client
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .unwrap()
        // Skip `Submitted` status
        .skip(1)
        .next()
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        status,
        TransactionStatus::PreconfirmationSuccess { .. }
    ));

    // When
    let script = vec![op::ret(RegId::ONE)];
    let script_data_to_change_tx_id = vec![123];
    let tx_with_same_input = client
        .assemble_script(
            script,
            script_data_to_change_tx_id,
            default_signing_wallet(),
        )
        .await
        .unwrap();
    let status = client.submit_and_await_commit(&tx_with_same_input).await;

    // Then
    let err = status.expect_err("Should receive error that transaction squeezed out");
    assert!(err.to_string().contains("was already spent"))
}

#[tokio::test]
async fn informs_immediately_if_the_input_was_spent_during_previous_blocks_without_timeout()
 {
    let mut config = config_with_fee();
    config.block_production = Trigger::Open {
        period: Duration::from_secs(1),
    };

    // Given
    config.txpool.pending_pool_tx_ttl = Duration::from_secs(1_000_000);

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let script = vec![op::ret(RegId::ONE)];
    let tx = client
        .assemble_script(script.clone(), vec![], default_signing_wallet())
        .await
        .unwrap();
    let different_tx_with_same_input = client
        .assemble_script(script, vec![123], default_signing_wallet())
        .await
        .unwrap();

    let status = client
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .unwrap()
        // Skip `Submitted` and `PreconfirmationSuccess` statuses
        .skip(2)
        .next()
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    let status = tokio::time::timeout(
        Duration::from_secs(10),
        client.submit_and_await_commit(&different_tx_with_same_input),
    )
    .await;

    // Then
    let status = status.expect("Should receive response from TxPool immediately");
    let err = status.expect_err("Should receive error that transaction squeezed out");
    assert!(err.to_string().contains("was already spent"))
}
