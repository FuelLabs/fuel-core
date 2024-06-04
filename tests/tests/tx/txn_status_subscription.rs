use std::time::Duration;

use fuel_core::{
    self,
    schema::tx::types::TransactionStatus,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::FuelClient;
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::{
        field::{
            Inputs,
            Outputs,
        },
        *,
    },
    fuel_types::ChainId,
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
    },
};
use futures::StreamExt;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

fn create_transaction<R: Rng>(rng: &mut R, script: Vec<Instruction>) -> Transaction {
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let script = script.into_iter().collect();
    let mut tx = TransactionBuilder::script(script, vec![])
        .script_gas_limit(10000)
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate.clone(),
            vec![],
        ))
        .add_input(Input::coin_predicate(
            rng.gen(),
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate,
            vec![],
        ))
        .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
        .add_output(Output::change(rng.gen(), 0, AssetId::default()))
        .finalize();
    tx.estimate_predicates(&CheckPredicateParams::default(), MemoryInstance::new())
        .expect("Predicate check failed");
    tx.into()
}

#[tokio::test]
async fn subscribe_txn_status() {
    let mut config = Config::local_node();
    config.block_production = fuel_core::service::config::Trigger::Interval {
        block_time: Duration::from_secs(2),
    };
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let gas_limit = 1_000_000;
    let maturity = Default::default();

    let create_script = |i: u8| {
        // The first two scripts will run and the rest will fail.
        let script = [op::addi(0x11 - i, 0x10, 1), op::ret(RegId::ONE)];
        let script: Vec<u8> = script
            .iter()
            .flat_map(|op| u32::from(*op).to_be_bytes())
            .collect();

        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        // The third transaction needs to have a different input.
        let utxo_id = if i == 2 { 2 } else { 1 };
        let utxo_id = UtxoId::new(Bytes32::from([utxo_id; 32]), 1);
        let coin_input = Input::coin_predicate(
            utxo_id,
            owner,
            1000,
            AssetId::zeroed(),
            TxPointer::default(),
            Default::default(),
            predicate,
            vec![],
        );
        let mut tx: Transaction = Transaction::script(
            gas_limit,
            script,
            vec![],
            policies::Policies::new()
                .with_maturity(maturity)
                .with_max_fee(0),
            vec![coin_input],
            vec![],
            vec![],
        )
        .into();
        // estimate predicate gas for coin_input predicate
        tx.estimate_predicates(&CheckPredicateParams::default(), MemoryInstance::new())
            .expect("should estimate predicate");

        tx
    };
    let txns: Vec<_> = (0..3).map(create_script).collect();
    let mut jhs = vec![];

    for (txn_idx, id) in txns.iter().map(|t| t.id(&ChainId::default())).enumerate() {
        let jh = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .subscribe_transaction_status(&id)
                    .await
                    .unwrap()
                    .enumerate()
                    .for_each(|(event_idx, r)| async move {
                        let r = r.unwrap();
                        match (txn_idx, event_idx) {
                            (0, 0) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Submitted{ .. }), "{r:?}"),
                            (0, 1) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::SqueezedOut{ .. }), "{r:?}"),
                            (1, 0) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Submitted{ .. }), "{r:?}"),
                            (1, 1) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Success{ .. }), "{r:?}"),
                            (2, 0) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Submitted{ .. }), "{r:?}"),
                            (2, 1) => assert!(matches!(r, fuel_core_client::client::types::TransactionStatus::Failure{ .. }), "{r:?}"),
                            _ => unreachable!("{} {} {:?}", txn_idx, event_idx, r),
                        }
                    })
                    .await;
            }
        });
        jhs.push(jh);
    }

    for tx in &txns {
        client.submit(tx).await.unwrap();
    }

    for jh in jhs {
        jh.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_regression_in_subscribe() {
    let mut rng = StdRng::seed_from_u64(11);
    let mut config = Config::local_node();
    config.utxo_validation = true;
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let node = FuelService::new_node(config).await.unwrap();
    let coin_pred = Input::coin_predicate(
        rng.gen(),
        owner,
        rng.gen(),
        rng.gen(),
        rng.gen(),
        Default::default(),
        predicate,
        vec![],
    );
    let contract = Input::contract(rng.gen(), rng.gen(), rng.gen(), rng.gen(), rng.gen());
    let contract_created = Output::contract_created(
        *contract.contract_id().unwrap(),
        *contract.state_root().unwrap(),
    );

    let mut empty_script =
        TransactionBuilder::script(vec![op::ret(0)].into_iter().collect(), vec![]);
    empty_script.script_gas_limit(100000);

    let empty_create = TransactionBuilder::create(rng.gen(), rng.gen(), vec![]);
    let txs = [
        empty_script.clone().add_input(coin_pred).finalize().into(),
        empty_create
            .clone()
            .add_input(contract.clone())
            .finalize()
            .into(),
        empty_create
            .clone()
            .add_input(contract.clone())
            .add_output(contract_created)
            .finalize()
            .into(),
        empty_create
            .clone()
            .add_output(contract_created)
            .finalize()
            .into(),
    ];
    for tx in txs {
        let model_result = submit_and_await_model(&tx);
        let r = node.submit_and_status_change(tx).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let r = match r {
            Ok(stream) => {
                let stream = stream
                    .filter(|status| {
                        futures::future::ready(!matches!(
                            status,
                            Ok(TransactionStatus::Submitted(_))
                        ))
                    })
                    .collect::<Vec<_>>();
                let mut r = tokio::time::timeout(Duration::from_secs(5), stream)
                    .await
                    .unwrap();
                r.pop().unwrap()
            }
            Err(e) => {
                assert!(!model_result, "{:?}", e);
                continue
            }
        };
        match r {
            Ok(s) => assert_eq!(
                matches!(s, TransactionStatus::Success(_)),
                model_result,
                "{:?}",
                s
            ),
            Err(e) => assert!(!model_result, "{:?}", e),
        }
    }
}

fn submit_and_await_model(tx: &Transaction) -> bool {
    match tx {
        Transaction::Script(script) => !script
            .inputs()
            .iter()
            .any(|i| i.is_coin() || i.is_message()),
        Transaction::Create(create) => {
            if create.inputs().iter().any(|o| o.is_contract())
                || create.inputs().is_empty()
            {
                false
            } else {
                create.outputs().iter().any(|o| o.is_contract_created())
            }
        }
        _ => true,
    }
}

#[tokio::test]
async fn txn_success_status_contains_receipts() {
    use fuel_core_client::client::types::TransactionStatus;

    // Given
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let chain_id = ChainId::default();
    let config = Config::local_node();
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // When
    let transaction_success = create_transaction(&mut rng, vec![op::ret(RegId::ONE)]);
    client.submit(&transaction_success).await.unwrap();
    let id = transaction_success.id(&chain_id);
    let status = client.await_transaction_commit(&id).await.unwrap();

    // Then
    if let TransactionStatus::Success { receipts, .. } = status {
        assert!(!receipts.is_empty());
        assert!(matches!(receipts[0], Receipt::Return { .. }));
        assert!(
            matches!(receipts[1], Receipt::ScriptResult { result, .. } if matches!(result, ScriptExecutionResult::Success))
        );
    } else {
        panic!("Expected successful transaction");
    };
}

#[tokio::test]
async fn txn_failure_status_contains_receipts() {
    use fuel_core_client::client::types::TransactionStatus;

    // Given
    let mut rng = StdRng::seed_from_u64(0xF00DF00D);
    let chain_id = ChainId::default();
    let config = Config::local_node();
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // When
    let transaction_failure = create_transaction(&mut rng, vec![op::rvrt(RegId::ONE)]);
    client.submit(&transaction_failure).await.unwrap();
    let id = transaction_failure.id(&chain_id);
    let status = client.await_transaction_commit(&id).await.unwrap();

    // Then
    if let TransactionStatus::Failure { receipts, .. } = status {
        assert!(!receipts.is_empty());
        assert!(matches!(receipts[0], Receipt::Revert { .. }));
        assert!(
            matches!(receipts[1], Receipt::ScriptResult { result, .. } if matches!(result, ScriptExecutionResult::Revert))
        );
    } else {
        panic!("Expected failed transaction");
    };
}
