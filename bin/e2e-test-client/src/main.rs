//! The `e2e-test-client` binary is used to run end-to-end tests for the Fuel client.

pub use fuel_core_e2e_client::*;
use fuel_core_e2e_client::{
    config::SuiteConfig,
    test_context::TestContext,
};
use libtest_mimic::{
    Arguments,
    Failed,
    Trial,
};
use std::{
    env,
    fs,
    future::Future,
};

pub mod tests;

fn main() {
    fn with_cloned(
        config: &SuiteConfig,
        f: impl FnOnce(SuiteConfig) -> anyhow::Result<(), Failed>,
    ) -> impl FnOnce() -> anyhow::Result<(), Failed> {
        let config = config.clone();
        move || f(config)
    }

    let mut args = Arguments::from_args();
    // If we run tests in parallel they may fail because try to use the same state like UTXOs.
    args.test_threads = Some(1);
    let config = load_config();

    let tests = vec![
        Trial::test(
            "can transfer from alice to bob",
            with_cloned(&config, |config| {
                let ctx = TestContext::new(config);
                async_execute(tests::transfers::basic_transfer(&ctx))
            }),
        ),
        Trial::test(
            "can execute script and get receipts",
            with_cloned(&config, |config| {
                let ctx = TestContext::new(config);
                async_execute(tests::script::receipts(&ctx))
            }),
        ),
        Trial::test(
            "can transfer from alice to bob and back",
            with_cloned(&config, |config| {
                let ctx = TestContext::new(config);
                async_execute(tests::transfers::transfer_back(&ctx))
            }),
        ),
        Trial::test(
            "can deploy a large contract",
            with_cloned(&config, |config| {
                let ctx = TestContext::new(config);
                async_execute(tests::contracts::deploy_large_contract(&ctx))
            }),
        ),
    ];

    libtest_mimic::run(&args, tests).exit();
}

fn load_config() -> SuiteConfig {
    // load from env var
    env::var_os(CONFIG_FILE_KEY)
        .map(|path| {
            let path = path.to_string_lossy().to_string();
            let file = fs::read(path).unwrap();
            toml::from_slice(&file).unwrap()
        })
        .unwrap_or_default()
}

fn async_execute<F: Future<Output = anyhow::Result<(), Failed>>>(
    func: F,
) -> Result<(), Failed> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(func)
}
