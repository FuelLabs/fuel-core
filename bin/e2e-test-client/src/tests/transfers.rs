use fuel_core_e2e_client::test_context::TestContext;
use libtest_mimic::Failed;

pub async fn basic_transfer(ctx: &TestContext) -> Result<(), Failed> {
    // alice makes transfer to bob
    let result = ctx.alice.transfer(ctx.bob.address, 100, None).await?;
    if !result.success {
        return Err("transfer failed".into())
    }

    tokio::time::sleep(ctx.config.wallet_delay).await;

    // bob checks to see if utxo was received
    let received_transfer = ctx.bob.owns_coin(result.transferred_utxo).await?;
    if !received_transfer {
        return Err("Bob failed to receive transfer".into())
    }

    Ok(())
}
