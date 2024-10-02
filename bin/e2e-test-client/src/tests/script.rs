use crate::test_context::{TestContext, BASE_AMOUNT};
use fuel_core_chain_config::{ContractConfig, SnapshotMetadata, StateConfig};
use fuel_core_types::{
    fuel_tx::{Receipt, ScriptExecutionResult, Transaction, UniqueIdentifier},
    fuel_types::{
        canonical::{Deserialize, Serialize},
        Salt,
    },
    services::executor::TransactionExecutionResult,
};
use futures::future::BoxFuture;
use futures::FutureExt;
use libtest_mimic::Failed;
use std::{
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::time::timeout;

// Executes transfer script and gets the receipts.
pub fn receipts(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        // Alice makes transfer to Bob
        let result = tokio::time::timeout(
            ctx.config.sync_timeout(),
            ctx.alice.transfer(ctx.bob.address, BASE_AMOUNT, None),
        )
        .await??;
        let status = result.status;
        if !result.success {
            return Err(format!("transfer failed with status {status:?}").into());
        }
        println!("The tx id of the script: {}", result.tx_id);

        let mut queries = vec![];
        for i in 0..100 {
            let ctx = ctx.clone();
            let tx_id = result.tx_id;
            queries.push(async move { (ctx.alice.client.receipts(&tx_id).await, i) });
        }

        let queries = futures::future::join_all(queries).await;
        for query in queries {
            let (query, query_number) = query;
            let receipts = query?;
            if receipts.is_none() {
                return Err(format!("Receipts are empty for query_number {query_number}").into());
            }
        }

        Ok(())
    }
    .boxed()
}

#[derive(PartialEq, Eq)]
enum DryRunResult {
    Successful,
    MayFail,
}

// Dry run the transaction.
pub fn dry_run(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        let transaction = tokio::time::timeout(
            ctx.config.sync_timeout(),
            ctx.alice.transfer_tx(ctx.bob.address, 0, None),
        )
        .await??;

        _dry_runs(ctx, vec![transaction], 100, DryRunResult::Successful).await
    }
    .boxed()
}

// Dry run multiple transactions
pub fn dry_run_multiple_txs(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        let transaction1 = tokio::time::timeout(
            ctx.config.sync_timeout(),
            ctx.alice.transfer_tx(ctx.bob.address, 0, None),
        )
        .await??;
        let transaction2 = tokio::time::timeout(
            ctx.config.sync_timeout(),
            ctx.alice.transfer_tx(ctx.alice.address, 0, None),
        )
        .await??;

        _dry_runs(
            ctx,
            vec![transaction1, transaction2],
            100,
            DryRunResult::Successful,
        )
        .await
    }
    .boxed()
}

fn load_contract(salt: Salt, path: impl AsRef<Path>) -> Result<ContractConfig, Failed> {
    let snapshot = SnapshotMetadata::read(path)?;
    let state_config = StateConfig::from_snapshot_metadata(snapshot)?;
    let mut contract_config = state_config
        .contracts
        .into_iter()
        .next()
        .ok_or("No contract found in the state")?;

    contract_config.update_contract_id(salt);
    Ok(contract_config)
}

// Maybe deploy a contract with large state and execute the script
pub fn run_contract_large_state(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        let salt: Salt = "0x3b91bab936e4f3db9453046b34c142514e78b64374bf61a04ab45afbd6bca83e"
            .parse()
            .expect("Should be able to parse the salt");
        let contract_config = load_contract(salt, "./src/tests/test_data/large_state")?;
        let dry_run = include_bytes!("test_data/large_state/tx.json");
        let dry_run: Transaction = serde_json::from_slice(dry_run.as_ref())
            .expect("Should be able to decode the Transaction");

        let contract_id = contract_config.contract_id;

        // If the contract is not deployed yet, let's deploy it
        let result = ctx.bob.client.contract(&contract_id).await;
        if result?.is_none() {
            let deployment_request = ctx.bob.deploy_contract(contract_config, salt);

            timeout(Duration::from_secs(20), deployment_request).await??;
        }

        _dry_runs(ctx, vec![dry_run], 100, DryRunResult::MayFail).await
    }
    .boxed()
}

pub fn arbitrary_transaction(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        const RAW_PATH: &str = "src/tests/test_data/arbitrary_tx.raw";
        const JSON_PATH: &str = "src/tests/test_data/arbitrary_tx.json";
        let dry_run_raw = std::fs::read_to_string(RAW_PATH).expect("Should read the raw transaction");
        let dry_run_json =
            std::fs::read_to_string(JSON_PATH).expect("Should read the json transaction");
        let bytes = dry_run_raw.replace("0x", "");
        let hex_tx = hex::decode(bytes).expect("Expected hex string");
        let dry_run_tx_from_raw: Transaction = Transaction::from_bytes(hex_tx.as_ref())
            .expect("Should be able to decode the Transaction from canonical representation");
        let mut dry_run_tx_from_json: Transaction = serde_json::from_str(dry_run_json.as_ref())
            .expect("Should be able to decode the Transaction from json representation");

        if std::env::var_os("OVERRIDE_RAW_WITH_JSON").is_some() {
            let bytes = dry_run_tx_from_json.to_bytes();
            std::fs::write(RAW_PATH, hex::encode(bytes)).expect("Should write the raw transaction");
        } else if std::env::var_os("OVERRIDE_JSON_WITH_RAW").is_some() {
            let bytes = serde_json::to_string_pretty(&dry_run_tx_from_raw)
                .expect("Should be able to encode the Transaction");
            dry_run_tx_from_json = dry_run_tx_from_raw.clone();
            std::fs::write(JSON_PATH, bytes.as_bytes()).expect("Should write the json transaction");
        }

        assert_eq!(dry_run_tx_from_raw, dry_run_tx_from_json);

        _dry_runs(ctx, vec![dry_run_tx_from_json], 100, DryRunResult::MayFail).await
    }
    .boxed()
}

fn _dry_runs(
    ctx: Arc<TestContext>,
    transactions: Vec<Transaction>,
    count: usize,
    expect: DryRunResult,
) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        println!("\nStarting dry runs");
        let mut queries = vec![];
        for i in 0..count {
            let ctx = ctx.clone();
            let transactions = transactions.clone();
            queries.push(async move {
                let before = tokio::time::Instant::now();
                let query = ctx
                    .alice
                    .client
                    .dry_run_opt(&transactions, Some(false), None)
                    .await;
                println!(
                    "Received the response for the query number {i} in {}ms",
                    before.elapsed().as_millis()
                );
                (query, i)
            });
        }

        // All queries should be resolved within 60 seconds.
        let queries =
            tokio::time::timeout(Duration::from_secs(60), futures::future::join_all(queries))
                .await?;

        let chain_info = ctx.alice.client.chain_info().await?;
        for query in queries {
            let (query, query_number) = query;
            if let Err(e) = &query {
                println!("The query {query_number} failed with {e}");
            }

            let tx_statuses = query?;
            for (tx_status, tx) in tx_statuses.iter().zip(transactions.iter()) {
                if tx_status.result.receipts().is_empty() {
                    return Err(format!("Receipts are empty for query_number {query_number}").into());
                }

                assert!(tx.id(&chain_info.consensus_parameters.chain_id()) == tx_status.id);
                if expect == DryRunResult::Successful {
                    assert!(matches!(
                        &tx_status.result,
                        TransactionExecutionResult::Success { result: _result, .. }
                    ));
                    assert!(matches!(
                        tx_status.result.receipts().last(),
                        Some(Receipt::ScriptResult {
                            result: ScriptExecutionResult::Success,
                            ..
                        })
                    ));
                }
            }
        }
        Ok(())
    }
    .boxed()
}
