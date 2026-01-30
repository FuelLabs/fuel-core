use std::num::NonZeroUsize;

#[derive(Clone, Debug)]
pub struct Config {
    /// The number of cores to use for the block execution.
    pub worker_count: NonZeroUsize,
    /// Enable metrics for the parallel executor.
    pub metrics: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worker_count: NonZeroUsize::new(1).expect("The value is not zero; qed"),
            metrics: false,
        }
    }
}
