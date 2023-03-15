use fuel_core_e2e_client::test_context::TestContext;
use libtest_mimic::Failed;
use tokio::time::timeout;

// deploy a large contract (7mb)
pub async fn deploy_large_contract(ctx: &TestContext) -> Result<(), Failed> {
    // generate large bytecode
    let bytecode = vec![0u8; 1_000_000];

    let deployment_request = ctx.bob.deploy_contract(bytecode);

    // wait for contract to deploy
    timeout(ctx.config.sync_timeout(), deployment_request).await??;

    Ok(())
}
