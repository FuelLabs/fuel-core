use clap::Args;
use fuel_core::service::config::ExecutorMode;
use fuel_core_types::blockchain::header::StateTransitionBytecodeVersion;

#[cfg(feature = "parallel-executor")]
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Args)]
pub struct ExecutorArgs {
    /// Overrides the version of the native executor.
    #[arg(long = "native-executor-version", env)]
    pub native_executor_version: Option<StateTransitionBytecodeVersion>,

    /// Executor mode to use.
    #[arg(long = "executor-mode", value_enum, default_value = "normal", env)]
    pub executor_mode: ExecutorMode,

    /// Number of cores to use for the parallel executor.
    #[cfg(feature = "parallel-executor")]
    #[arg(
        long = "executor-number-of-cores",
        env,
        default_value = "1",
        alias = "executor-worker-count"
    )]
    pub executor_number_of_cores: NonZeroUsize,

    /// Enable metrics for the parallel executor.
    #[cfg(feature = "parallel-executor")]
    #[arg(long = "executor-metrics", env)]
    pub executor_metrics: bool,
}
