use crate::test_context::TestContext;
use fuel_core_chain_config::ContractConfig;
use fuel_core_types::fuel_crypto::rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use libtest_mimic::Failed;
use std::time::Duration;
use tokio::time::timeout;

// deploy a large contract (16mb)
pub async fn deploy_large_contract(ctx: &TestContext) -> Result<(), Failed> {
    // generate large bytecode
    let bytecode = if ctx.config.full_test {
        // 16mib takes a long time to process due to contract root calculations
        // it is under an optional flag to avoid slowing down every CI run.
        vec![0u8; 16 * 1024 * 1024]
    } else {
        vec![0u8; 1024 * 1024]
    };
    let mut rng = StdRng::seed_from_u64(2222);
    let mut contract_config = ContractConfig {
        contract_id: Default::default(),
        code: bytecode,
        salt: rng.gen(),
        state: None,
        balances: None,
        tx_id: None,
        output_index: None,
        tx_pointer_block_height: None,
        tx_pointer_tx_idx: None,
    };
    contract_config.calculate_contract_id();

    let deployment_request = ctx.bob.deploy_contract(contract_config);

    // wait for contract to deploy in 5 minutes, because 16mb takes a lot of time.
    timeout(Duration::from_secs(300), deployment_request).await??;

    Ok(())
}
