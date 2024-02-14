use crate::test_context::{
    TestContext,
    BASE_AMOUNT,
};
use fuel_core_chain_config::ContractConfig;
use fuel_core_types::{
    fuel_tx::{
        field::{
            GasPrice,
            ScriptGasLimit,
        },
        Receipt,
        ScriptExecutionResult,
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::canonical::Deserialize,
    services::executor::TransactionExecutionResult,
};
use libtest_mimic::Failed;
use std::time::Duration;
use tokio::time::timeout;

// Executes transfer script and gets the receipts.
pub async fn receipts(ctx: &TestContext) -> Result<(), Failed> {
    // alice makes transfer to bob
    let result = tokio::time::timeout(
        ctx.config.sync_timeout(),
        ctx.alice.transfer(ctx.bob.address, BASE_AMOUNT, None),
    )
    .await??;
    let status = result.status;
    if !result.success {
        return Err(format!("transfer failed with status {status:?}").into())
    }
    println!("The tx id of the script: {}", result.tx_id);

    let mut queries = vec![];
    for i in 0..100 {
        let tx_id = result.tx_id;
        queries.push(async move { (ctx.alice.client.receipts(&tx_id).await, i) });
    }

    let queries = futures::future::join_all(queries).await;
    for query in queries {
        let (query, query_number) = query;
        let receipts = query?;
        if receipts.is_none() {
            return Err(
                format!("Receipts are empty for query_number {query_number}").into(),
            )
        }
    }

    Ok(())
}

#[derive(PartialEq, Eq)]
enum DryRunResult {
    Successful,
    MayFail,
}

// Dry run the transaction.
pub async fn dry_run(ctx: &TestContext) -> Result<(), Failed> {
    let transaction = tokio::time::timeout(
        ctx.config.sync_timeout(),
        ctx.alice.transfer_tx(ctx.bob.address, 0, None),
    )
    .await??;

    _dry_runs(ctx, &[transaction], 1000, DryRunResult::Successful).await
}

// Dry run multiple transactions
pub async fn dry_run_multiple_txs(ctx: &TestContext) -> Result<(), Failed> {
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
        &[transaction1, transaction2],
        1000,
        DryRunResult::Successful,
    )
    .await
}

// Maybe deploy a contract with large state and execute the script
pub async fn run_contract_large_state(ctx: &TestContext) -> Result<(), Failed> {
    let contract_config = include_bytes!("test_data/large_state/contract.json");
    let mut contract_config: ContractConfig =
        serde_json::from_slice(contract_config.as_ref())
            .expect("Should be able do decode the ContractConfig");
    let dry_run = include_bytes!("test_data/large_state/tx.json");
    let dry_run: Transaction = serde_json::from_slice(dry_run.as_ref())
        .expect("Should be able do decode the Transaction");

    // If the contract changed, you need to update the
    // `f4292fe50d21668e140636ab69c7d4b3d069f66eb9ef3da4b0a324409cc36b8c` in the
    // `test_data/large_state/contract.json` together with:
    // 244, 41, 47, 229, 13, 33, 102, 142, 20, 6, 54, 171, 105, 199, 212, 179, 208, 105, 246, 110, 185, 239, 61, 164, 176, 163, 36, 64, 156, 195, 107, 140,
    contract_config.calculate_contract_id();
    let contract_id = contract_config.contract_id;
    println!("\nThe `contract_id` of the contract with large state: {contract_id}");

    // if the contract is not deployed yet, let's deploy it
    let result = ctx.bob.client.contract(&contract_id).await;
    if result?.is_none() {
        let deployment_request = ctx.bob.deploy_contract(contract_config);

        // wait for contract to deploy in 300 seconds because `state_root` calculation is too long.
        // https://github.com/FuelLabs/fuel-core/issues/1143
        timeout(Duration::from_secs(300), deployment_request).await??;
    }

    _dry_runs(ctx, &[dry_run], 1000, DryRunResult::MayFail).await
}

// Send non specific transaction from `non_specific_tx.raw` file
pub async fn non_specific_transaction(ctx: &TestContext) -> Result<(), Failed> {
    let dry_run = include_str!("test_data/non_specific_tx.raw");
    let bytes = dry_run.replace("0x", "");
    let hex_tx = hex::decode(bytes).expect("Expected hex string");
    let mut dry_run: Transaction = Transaction::from_bytes(hex_tx.as_ref())
        .expect("Should be able do decode the Transaction");

    if let Some(script) = dry_run.as_script_mut() {
        *script.script_gas_limit_mut() = 100000;
        script.set_gas_price(0);
    }

    _dry_runs(ctx, &[dry_run], 1000, DryRunResult::MayFail).await
}

async fn _dry_runs(
    ctx: &TestContext,
    transactions: &[Transaction],
    count: usize,
    expect: DryRunResult,
) -> Result<(), Failed> {
    println!("\nStarting dry runs");
    let mut queries = vec![];
    for i in 0..count {
        queries.push(async move {
            let before = tokio::time::Instant::now();
            let query = ctx
                .alice
                .client
                .dry_run_opt(transactions, Some(false))
                .await;
            println!(
                "Received the response for the query number {i} for {}ms",
                before.elapsed().as_millis()
            );
            (query, i)
        });
    }

    // All queries should be resolved for 60 seconds.
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
                return Err(
                    format!("Receipts are empty for query_number {query_number}").into(),
                )
            }

            assert!(tx.id(&chain_info.consensus_parameters.chain_id) == tx_status.id);
            if expect == DryRunResult::Successful {
                assert!(matches!(
                    &tx_status.result,
                    TransactionExecutionResult::Success {
                        result: _result,
                        ..
                    }
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
