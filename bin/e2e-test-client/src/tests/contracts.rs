use fuel_core_e2e_client::test_context::TestContext;
use libtest_mimic::Failed;
use std::time::Duration;
use tokio::time::timeout;

// deploy a large contract (16mb)
pub async fn deploy_large_contract(ctx: &TestContext) -> Result<(), Failed> {
    // generate large bytecode
    let bytecode = vec![0u8; 16 * 1024 * 1024];

    let deployment_request = ctx.bob.deploy_contract(bytecode);

    // wait for contract to deploy in 5 minutes, because 16mb takes a lot of time.
    timeout(Duration::from_secs(300), deployment_request).await??;

    Ok(())
}
