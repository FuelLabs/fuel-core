use crate::test_context::{
    TestContext,
    BASE_AMOUNT,
};
use libtest_mimic::Failed;

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
        let tx_id = result.tx_id.to_string();
        queries.push(async move { (ctx.alice.client.receipts(tx_id.as_str()).await, i) });
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

// Dry run the transaction.
pub async fn dry_run(ctx: &TestContext) -> Result<(), Failed> {
    let transaction = tokio::time::timeout(
        ctx.config.sync_timeout(),
        ctx.alice.transfer_tx(ctx.bob.address, 0, None),
    )
    .await??;

    println!("\nStarting dry runs");
    let mut queries = vec![];
    for i in 0..1000 {
        let tx = transaction.clone();
        let alice = ctx.alice.clone();
        queries.push(async move {
            let before = tokio::time::Instant::now();
            let query = alice.client.dry_run(&tx).await;
            println!(
                "Received the response for the query number {i} for {}ms",
                before.elapsed().as_millis()
            );
            (query, i)
        });
    }

    // All queries should be resolved for 30 seconds.
    let queries = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        futures::future::join_all(queries),
    )
    .await?;
    for query in queries {
        let (query, query_number) = query;
        let receipts = query?;
        if receipts.is_empty() {
            return Err(
                format!("Receipts are empty for query_number {query_number}").into(),
            )
        }
    }

    Ok(())
}
