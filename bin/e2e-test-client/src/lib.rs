use crate::{
    config::SuiteConfig,
    test_context::TestContext,
};
use futures::future::BoxFuture;
use futures::FutureExt; // For using the `.boxed()` method
use libtest_mimic::{
    Arguments,
    Failed,
    Trial,
};
use std::{
    env,
    fs,
    future::Future,
    sync::Arc,
    time::Duration,
};

pub const CONFIG_FILE_KEY: &str = "FUEL_CORE_E2E_CONFIG";
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(10);

pub mod config;
pub mod test_context;
pub mod tests;

pub fn main_body(config: SuiteConfig, mut args: Arguments) {
    // If we run tests in parallel they may fail because they try to use the same state like UTXOs.
    args.test_threads = Some(1);

    let tests = vec![
        create_test("can transfer from alice to bob", &config, tests::transfers::basic_transfer),
        create_test(
            "can transfer from alice to bob and back",
            &config,
            tests::transfers::transfer_back,
        ),
        create_test("can collect fee from alice", &config, tests::collect_fee::collect_fee),
        create_test("can execute script and get receipts", &config, tests::script::receipts),
        create_test(
            "can dry run transfer script and get receipts",
            &config,
            tests::script::dry_run,
        ),
        create_test(
            "can dry run multiple transfer scripts and get receipts",
            &config,
            tests::script::dry_run_multiple_txs,
        ),
        create_test(
            "dry run script that touches the contract with large state",
            &config,
            tests::script::run_contract_large_state,
        ),
        create_test(
            "dry run transaction from `arbitrary_tx.raw` file",
            &config,
            tests::script::arbitrary_transaction,
        ),
        create_test(
            "can deploy a large contract",
            &config,
            tests::contracts::deploy_large_contract,
        ),
    ];

    libtest_mimic::run(&args, tests).exit();
}

// Helper function to reduce code duplication when creating tests
fn create_test<F>(
    name: &'static str,
    config: &SuiteConfig,
    test_fn: F,
) -> Trial
where
    F: FnOnce(Arc<TestContext>) -> BoxFuture<'static, anyhow::Result<(), Failed>>
        + Send
        + Sync
        + 'static,
{
    let cloned_config = config.clone();
    Trial::test(
        name,
        move || async_execute(async move {
            let ctx = Arc::new(TestContext::new(cloned_config).await);
            test_fn(ctx).await
        }),
    )
}

pub fn load_config_env() -> SuiteConfig {
    // Load from environment variable
    env::var_os(CONFIG_FILE_KEY)
        .map(|path| load_config(path.to_string_lossy().to_string()))
        .unwrap_or_default()
}

pub fn load_config(path: String) -> SuiteConfig {
    let file = fs::read(path).unwrap();
    toml::from_slice(&file).unwrap()
}

fn async_execute<F: Future<Output = anyhow::Result<(), Failed>>>(func: F) -> Result<(), Failed> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(func)
}
