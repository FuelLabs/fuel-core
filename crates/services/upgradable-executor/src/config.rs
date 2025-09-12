use fuel_core_executor::executor::ExecutionOptions;
use fuel_core_types::blockchain::header::StateTransitionBytecodeVersion;

#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Default mode for `forbid_fake_coins` in `ExecutionOptions`.
    pub forbid_fake_coins_default: bool,
    /// Allow usage of syscall during the execution of the transactions.
    pub allow_syscall: bool,
    /// The version of the native executor to determine usage of native vs WASM executor.
    /// If it is `None`, the `Executor::VERSION` is used.
    ///
    /// When a block version matches the native executor version, we use
    /// the native executor; otherwise, we use the WASM executor.
    pub native_executor_version: Option<StateTransitionBytecodeVersion>,
    /// Allow execution using blocks in the past.
    /// This is rather expensive and not needed for most use cases, so it can be disabled.
    pub allow_historical_execution: bool,
}

impl From<&Config> for ExecutionOptions {
    fn from(value: &Config) -> Self {
        Self {
            forbid_fake_coins: value.forbid_fake_coins_default,
            allow_syscall: value.allow_syscall,
        }
    }
}
