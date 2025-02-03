use fuel_core_upgradable_executor::config::Config as ExecutorConfig;
use std::num::NonZeroUsize;

#[derive(Clone, Debug)]
pub struct Config {
    /// The number of cores to use for the block execution.
    pub number_of_cores: NonZeroUsize,
    /// See [`fuel_core_upgradable_executor::config::Config`].
    pub executor_config: ExecutorConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            number_of_cores: NonZeroUsize::new(1).expect("The value is not zero; qed"),
            executor_config: Default::default(),
        }
    }
}
