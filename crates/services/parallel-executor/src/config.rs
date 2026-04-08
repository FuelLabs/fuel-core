use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, Default)]
pub enum WorkerCountPolicy {
    #[default]
    StaticMax,
    DynamicIdle,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The number of cores to use for the block execution.
    pub worker_count: NonZeroUsize,
    /// How to choose worker count used for tx selection requests.
    pub worker_count_policy: WorkerCountPolicy,
    /// Enable metrics for the parallel executor.
    pub metrics: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worker_count: NonZeroUsize::new(1).expect("The value is not zero; qed"),
            worker_count_policy: WorkerCountPolicy::StaticMax,
            metrics: false,
        }
    }
}
