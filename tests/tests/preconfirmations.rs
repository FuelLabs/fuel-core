use std::time::Duration;

use fuel_core::service::Config;
use fuel_core_bin::FuelService;
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_tx::{
        Address,
        AssetId,
        Output,
        Receipt,
        TransactionBuilder,
        TxPointer,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
    fuel_vm::SecretKey,
};
use futures::StreamExt;
use rand::Rng;
use test_helpers::{
    assemble_tx::AssembleAndRunTx,
    config_with_fee,
    default_signing_wallet,
};

#[tokio::test]
async fn preconfirmation__received_after_successful_execution() {
    let mut config = config_with_fee();
    config.block_production = Trigger::Never;

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let script = vec![
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::ret(RegId::ONE),
    ];
    let tx = client
        .assemble_script(script, vec![], default_signing_wallet())
        .await
        .unwrap();

    let tx_id = tx.id(&Default::default());
    let mut tx_statuses_subscriber = client
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .unwrap();

    // When
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    client.produce_blocks(1, None).await.unwrap();
    if let TransactionStatus::PreconfirmationSuccess {
        tx_pointer,
        total_fee: _,
        total_gas: _,
        transaction_id,
        receipts,
        resolved_outputs,
    } = tx_statuses_subscriber.next().await.unwrap().unwrap()
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 1));
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 3);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Return {
                val, ..
            } if val == 1));
        let outputs = resolved_outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].output.is_change());
    } else {
        panic!("Expected preconfirmation status");
    }
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { .. }
    ));
}

#[tokio::test]
async fn preconfirmation__received_when_asked() {
    let mut config = config_with_fee();
    config.block_production = Trigger::Never;

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let script = vec![op::ret(RegId::ONE)];
    let tx = client
        .assemble_script(script, vec![], default_signing_wallet())
        .await
        .unwrap();

    // When
    let mut tx_statuses_subscriber = client
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .unwrap();

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    client.produce_blocks(1, None).await.unwrap();
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::PreconfirmationSuccess { .. }
    ));
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { block_height, .. } if block_height == BlockHeight::new(1)
    ));
}

#[tokio::test]
async fn preconfirmation__not_received_when_not_asked() {
    let mut config = config_with_fee();
    config.block_production = Trigger::Never;

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let script = vec![op::ret(RegId::ONE)];
    let tx = client
        .assemble_script(script, vec![], default_signing_wallet())
        .await
        .unwrap();

    let tx_id = tx.id(&Default::default());
    // When
    let mut tx_statuses_update_subscriber =
        client.subscribe_transaction_status(&tx_id).await.unwrap();
    let mut tx_statuses_subscriber = client
        .submit_and_await_status_opt(&tx, None, None)
        .await
        .unwrap();

    // Then
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    assert!(matches!(
        tx_statuses_update_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    client.produce_blocks(1, None).await.unwrap();
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { block_height, .. } if block_height == BlockHeight::new(1)
    ));
    assert!(matches!(
        tx_statuses_update_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Success { block_height, .. } if block_height == BlockHeight::new(1)
    ));
}

#[tokio::test]
async fn preconfirmation__received_after_failed_execution() {
    let mut config = config_with_fee();
    config.block_production = Trigger::Never;

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let script = vec![
        op::addi(0x10, RegId::ZERO, 0xca),
        op::addi(0x11, RegId::ZERO, 0xba),
        op::log(0x10, 0x11, RegId::ZERO, RegId::ZERO),
        op::rvrt(RegId::ONE),
        op::ret(RegId::ONE),
    ];
    let tx = client
        .assemble_script(script, vec![], default_signing_wallet())
        .await
        .unwrap();

    let tx_id = tx.id(&Default::default());
    let mut tx_statuses_subscriber = client
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .unwrap();

    // When
    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    client.produce_blocks(1, None).await.unwrap();
    if let TransactionStatus::PreconfirmationFailure {
        tx_pointer,
        total_fee: _,
        total_gas: _,
        transaction_id,
        receipts,
        resolved_outputs,
        reason: _,
    } = tx_statuses_subscriber.next().await.unwrap().unwrap()
    {
        // Then
        assert_eq!(tx_pointer, TxPointer::new(BlockHeight::new(1), 1));
        assert_eq!(transaction_id, tx_id);
        let receipts = receipts.unwrap();
        assert_eq!(receipts.len(), 3);
        assert!(matches!(receipts[0],
            Receipt::Log {
                ra, rb, ..
            } if ra == 0xca && rb == 0xba));

        assert!(matches!(receipts[1],
            Receipt::Revert {
                ra, ..
            } if ra == 1));
        let outputs = resolved_outputs.unwrap();
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].output.is_change());
    } else {
        panic!("Expected preconfirmation status");
    }

    assert!(matches!(
        tx_statuses_subscriber.next().await.unwrap().unwrap(),
        TransactionStatus::Failure { .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn preconfirmation__received_tx_inserted_end_block_open_period() {
    let mut config = config_with_fee();
    let block_production_period = Duration::from_secs(1);

    config.block_production = Trigger::Open {
        period: block_production_period,
    };
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let script = vec![op::ret(RegId::ONE)];
    let tx = client
        .assemble_script(script, vec![], default_signing_wallet())
        .await
        .unwrap();

    // When
    client
        .submit_and_await_status_opt(&tx, None, Some(true))
        .await
        .unwrap()
        .enumerate()
        .for_each(|(event_idx, r)| async move {
            let r = r.unwrap();
            // Then
            match (event_idx, r) {
                (0, TransactionStatus::Submitted { .. }) => {}
                (1, TransactionStatus::PreconfirmationSuccess { .. }) => {}
                (2, TransactionStatus::Success { block_height, .. }) => {
                    assert_eq!(block_height, BlockHeight::new(1));
                }
                (_, r) => panic!("Unexpected event: {:?}", r),
            }
        })
        .await;
}

#[tokio::test]
async fn preconfirmation__received_after_execution__multiple_txs() {
    let mut rng = rand::thread_rng();
    let mut config = Config::local_node();
    config.block_production = Trigger::Never;

    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Given
    let tx1 = TransactionBuilder::script(
        vec![op::ret(RegId::ONE)].into_iter().collect(),
        vec![],
    )
    .script_gas_limit(1_000_000)
    .add_unsigned_coin_input(
        SecretKey::random(&mut rng),
        rng.gen(),
        10,
        AssetId::default(),
        Default::default(),
    )
    .add_output(Output::variable(
        Address::new([0; 32]),
        0,
        AssetId::default(),
    ))
    .finalize_as_transaction();
    let tx2 = TransactionBuilder::script(
        vec![op::ret(RegId::ONE)].into_iter().collect(),
        vec![1, 2, 3],
    )
    .script_gas_limit(1_000_000)
    .add_unsigned_coin_input(
        SecretKey::random(&mut rng),
        rng.gen(),
        10,
        AssetId::default(),
        Default::default(),
    )
    .add_output(Output::variable(
        Address::new([0; 32]),
        0,
        AssetId::default(),
    ))
    .finalize_as_transaction();

    // Given
    let mut tx_statuses_subscriber1 = client
        .submit_and_await_status_opt(&tx1, None, Some(true))
        .await
        .unwrap();
    let mut tx_statuses_subscriber2 = client
        .submit_and_await_status_opt(&tx2, None, Some(true))
        .await
        .unwrap();

    // When
    assert!(matches!(
        tx_statuses_subscriber1.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    assert!(matches!(
        tx_statuses_subscriber2.next().await.unwrap().unwrap(),
        TransactionStatus::Submitted { .. }
    ));
    client.produce_blocks(1, None).await.unwrap();
    assert!(matches!(
        tx_statuses_subscriber1.next().await.unwrap().unwrap(),
        TransactionStatus::PreconfirmationSuccess { .. }
    ));
    assert!(matches!(
        tx_statuses_subscriber2.next().await.unwrap().unwrap(),
        TransactionStatus::PreconfirmationSuccess { .. }
    ));
    // Then
    assert!(matches!(
        tx_statuses_subscriber1.next().await.unwrap().unwrap(),
        TransactionStatus::Success { block_height, .. } if block_height == BlockHeight::new(1)
    ));
    assert!(matches!(
        tx_statuses_subscriber2.next().await.unwrap().unwrap(),
        TransactionStatus::Success { block_height, .. } if block_height == BlockHeight::new(1)
    ));
}
