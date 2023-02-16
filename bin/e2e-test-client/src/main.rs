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
    let args = Arguments::from_args();

    let config = load_config();
    let ctx = TestContext::new(config);

    let tests = vec![Trial::test("can transfer from alice to bob", move || {
        async_execute(tests::transfers::basic_transfer(&ctx))
    })];

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
