use crate::test_context::{TestContext, BASE_AMOUNT};
use futures::future::BoxFuture;
use futures::FutureExt;
use libtest_mimic::Failed;
use std::sync::Arc;
use tokio::time::timeout;

// Alice makes transfer to Bob of `4 * BASE_AMOUNT` native tokens.
pub fn basic_transfer(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        // Alice makes transfer to Bob
        let result = ctx
            .alice
            .transfer(ctx.bob.address, 4 * BASE_AMOUNT, None)
            .await?;
        if !result.success {
            return Err("transfer failed".into());
        }
        // Wait until Bob sees the transaction
        timeout(
            ctx.config.sync_timeout(),
            ctx.bob.client.await_transaction_commit(&result.tx_id),
        )
        .await??;
        println!("\nThe tx id of the transfer: {}", result.tx_id);

        // Bob checks to see if UTXO was received
        // We don't check balance to avoid brittleness in the case of external activity on these wallets
        let received_transfer = ctx.bob.owns_coin(result.transferred_utxo).await?;
        if !received_transfer {
            return Err("Bob failed to receive transfer".into());
        }

        Ok(())
    }
    .boxed()
}

// Alice makes transfer to Bob of `4 * BASE_AMOUNT` native tokens - `basic_transfer`.
// Bob returns `3 * BASE_AMOUNT` tokens back to Alice and pays `BASE_AMOUNT` as a fee.
pub fn transfer_back(ctx: Arc<TestContext>) -> BoxFuture<'static, Result<(), Failed>> {
    async move {
        basic_transfer(ctx.clone()).await?;

        // Bob makes transfer to Alice
        let result = ctx
            .bob
            .transfer(ctx.alice.address, 3 * BASE_AMOUNT, None)
            .await?;
        if !result.success {
            return Err("transfer failed".into());
        }
        // Wait until Alice sees the transaction
        timeout(
            ctx.config.sync_timeout(),
            ctx.alice.client.await_transaction_commit(&result.tx_id),
        )
        .await??;

        // Alice checks to see if UTXO was received
        // We don't check balance to avoid brittleness in the case of external activity on these wallets
        let received_transfer = ctx.alice.owns_coin(result.transferred_utxo).await?;
        if !received_transfer {
            return Err("Alice failed to receive transfer".into());
        }

        Ok(())
    }
    .boxed()
}
