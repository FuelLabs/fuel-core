//! Tests involving client behavior when utxo-validation is enabled

use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::TransactionStatus,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::*,
    fuel_tx::*,
    fuel_types::ChainId,
};
use futures::future::join_all;
use itertools::Itertools;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::collections::BTreeSet;

#[tokio::test]
async fn submit_utxo_verified_tx_with_min_gas_price() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![], vec![], None);
    // initialize 10 random transactions that transfer coins and call a contract
    let transactions = (1..=10)
        .map(|i| {
            TransactionBuilder::script(
                op::ret(RegId::ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                SecretKey::random(&mut rng),
                rng.gen(),
                1000 + i,
                Default::default(),
                Default::default(),
            )
            .add_input(Input::contract(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                contract_id,
            ))
            .add_output(Output::change(rng.gen(), 0, AssetId::default()))
            .add_output(Output::contract(1, Default::default(), Default::default()))
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions.iter().collect_vec());

    // spin up node
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    // submit transactions and verify their status
    for tx in transactions {
        let tx = tx.into();
        client.submit_and_await_commit(&tx).await.unwrap();
        // verify that the tx returned from the api matches the submitted tx
        let ret_tx = client
            .transaction(&tx.id(&ChainId::default()))
            .await
            .unwrap()
            .unwrap()
            .transaction;

        let transaction_result = client
            .transaction_status(&ret_tx.id(&ChainId::default()))
            .await
            .ok()
            .unwrap();

        if let TransactionStatus::Success { block_height, .. } =
            transaction_result.clone()
        {
            let block_exists = client.block_by_height(block_height).await.unwrap();

            assert!(block_exists.is_some());
        }

        // Once https://github.com/FuelLabs/fuel-core/issues/50 is resolved this should rely on the Submitted Status rather than Success
        assert!(matches!(
            transaction_result,
            TransactionStatus::Success { .. }
        ));
    }
}

#[tokio::test]
async fn submit_utxo_verified_tx_below_min_gas_price_fails() {
    // initialize transaction
    let tx = TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .add_random_fee_input()
    .script_gas_limit(100)
    .finalize_as_transaction();

    // initialize node with higher minimum gas price
    let mut test_builder = TestSetupBuilder::new(2322u64);
    test_builder.starting_gas_price = 10;
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let result = client.submit(&tx).await;

    assert!(result.is_err());
    let error = result.err().unwrap().to_string();
    assert!(error.contains("InsufficientMaxFee"));
}

// verify that dry run can disable utxo_validation by simulating a transaction with unsigned
// non-existent coin inputs
#[tokio::test]
async fn dry_run_override_utxo_validation() {
    let mut rng = StdRng::seed_from_u64(2322);

    let asset_id = rng.gen();
    let tx = TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .script_gas_limit(10000)
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        1000,
        AssetId::default(),
        Default::default(),
        0,
    ))
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        rng.gen(),
        asset_id,
        Default::default(),
        0,
    ))
    .add_output(Output::change(rng.gen(), 0, asset_id))
    .add_witness(Default::default())
    .finalize_as_transaction();

    let context = TestSetupBuilder::new(2322).finalize().await;

    let tx_statuses = context
        .client
        .dry_run_opt(&[tx], Some(false), None)
        .await
        .unwrap();
    let log = tx_statuses
        .last()
        .expect("Nonempty response")
        .result
        .receipts();
    assert_eq!(2, log.len());

    assert!(matches!(log[0],
        Receipt::Return {
            val, ..
        } if val == 1));
}

// verify that dry run without utxo-validation override respects the node setting
#[tokio::test]
async fn dry_run_no_utxo_validation_override() {
    let mut rng = StdRng::seed_from_u64(2322);

    let asset_id = rng.gen();
    // construct a tx with invalid inputs
    let tx = TransactionBuilder::script(
        op::ret(RegId::ONE).to_bytes().into_iter().collect(),
        vec![],
    )
    .script_gas_limit(1000)
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        1000,
        AssetId::default(),
        Default::default(),
        0,
    ))
    .add_input(Input::coin_signed(
        rng.gen(),
        rng.gen(),
        rng.gen(),
        asset_id,
        Default::default(),
        0,
    ))
    .add_output(Output::change(rng.gen(), 0, asset_id))
    .add_witness(Default::default())
    .finalize_as_transaction();

    let client = TestSetupBuilder::new(2322).finalize().await.client;

    // verify that the client validated the inputs and failed the tx
    let res = client.dry_run_opt(&[tx], None, None).await;
    assert!(res.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_tx_submission_produces_expected_blocks() {
    const TEST_TXS: i32 = 10;

    let mut rng = StdRng::seed_from_u64(2322u64);
    let mut test_builder = TestSetupBuilder::new(100);

    // generate random txs
    let secret = SecretKey::random(&mut rng);
    let txs = (0..TEST_TXS)
        .map(|i| {
            TransactionBuilder::script(
                op::ret(RegId::ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                secret,
                rng.gen(),
                rng.gen_range((100000 + i as u64)..(200000 + i as u64)),
                Default::default(),
                Default::default(),
            )
            .add_output(Output::change(rng.gen(), 0, Default::default()))
            .finalize()
        })
        .collect_vec();

    // collect all tx ids
    let tx_ids: BTreeSet<_> = txs.iter().map(|tx| tx.id(&ChainId::default())).collect();

    // setup the genesis coins for spending
    test_builder.config_coin_inputs_from_transactions(&txs.iter().collect_vec());
    let txs: Vec<Transaction> = txs.into_iter().map(Into::into).collect_vec();

    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let tasks = txs.iter().map(|tx| client.submit_and_await_commit(tx));

    let tx_status: Vec<TransactionStatus> = join_all(tasks)
        .await
        .into_iter()
        .try_collect()
        .expect("expected successful transactions");

    // assert all txs are successful
    for status in tx_status {
        assert!(
            matches!(status, TransactionStatus::Success { .. }),
            "expected successful tx status, got: {status:?}"
        );
    }

    let total_blocks = client
        .blocks(PaginationRequest {
            results: TEST_TXS * 2,
            direction: PageDirection::Forward,
            cursor: None,
        })
        .await
        .unwrap();

    // ensure block heights are all unique
    let deduped = total_blocks
        .results
        .iter()
        .map(|b| b.header.height)
        .dedup()
        .collect_vec();

    // ensure all transactions are included across all the blocks
    let included_txs: BTreeSet<Bytes32> = total_blocks
        .results
        .iter()
        .flat_map(|b| {
            b.transactions
                .iter()
                .take(b.transactions.len().saturating_sub(1) /* coinbase */)
                .copied()
        })
        .dedup_with_count()
        .map(|(count, id)| {
            assert_eq!(count, 1, "duplicate tx detected {id}");
            id
        })
        .collect();

    assert_eq!(total_blocks.results.len(), deduped.len());
    assert_eq!(included_txs, tx_ids);
}
