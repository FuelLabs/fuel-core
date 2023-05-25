use crate::test_context::{
    TestContext,
    BASE_AMOUNT,
};
use libtest_mimic::Failed;
use tokio::time::timeout;

// Alice makes transfer to Bob of `4 * BASE_AMOUNT` native tokens.
pub async fn basic_transfer(ctx: &TestContext) -> Result<(), Failed> {
    // alice makes transfer to bob
    let result = ctx
        .alice
        .transfer(ctx.bob.address, 4 * BASE_AMOUNT, None)
        .await?;
    if !result.success {
        return Err("transfer failed".into())
    }
    // wait until bob sees the transaction
    timeout(
        ctx.config.sync_timeout(),
        ctx.bob.client.await_transaction_commit(&result.tx_id),
    )
    .await??;
    println!("\nThe tx id of the transfer: {}", result.tx_id);

    // bob checks to see if utxo was received
    // we don't check balance in order to avoid brittleness in the case of
    // external activity on these wallets
    let received_transfer = ctx.bob.owns_coin(result.transferred_utxo).await?;
    if !received_transfer {
        return Err("Bob failed to receive transfer".into())
    }

    Ok(())
}

// Alice makes transfer to Bob of `4 * BASE_AMOUNT` native tokens - `basic_transfer`.
// Bob returns `3 * BASE_AMOUNT` tokens back to Alice and pays `BASE_AMOUNT` as a fee.
pub async fn transfer_back(ctx: &TestContext) -> Result<(), Failed> {
    basic_transfer(ctx).await?;

    // bob makes transfer to alice
    let result = ctx
        .bob
        .transfer(ctx.alice.address, 3 * BASE_AMOUNT, None)
        .await?;
    if !result.success {
        return Err("transfer failed".into())
    }
    // wait until alice sees the transaction
    timeout(
        ctx.config.sync_timeout(),
        ctx.alice.client.await_transaction_commit(&result.tx_id),
    )
    .await??;

    // alice checks to see if utxo was received
    // we don't check balance in order to avoid brittleness in the case of
    // external activity on these wallets
    let received_transfer = ctx.alice.owns_coin(result.transferred_utxo).await?;
    if !received_transfer {
        return Err("Alice failed to receive transfer".into())
    }

    Ok(())
}
