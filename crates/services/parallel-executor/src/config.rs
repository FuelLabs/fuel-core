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
    /// Whether to validate the UTXO set (the node's `utxo_validation` flag).
    ///
    /// When `false` (a supported debugging mode), input coins are NOT required
    /// to exist in the database: the sequential executor fabricates missing
    /// coins from the input's own fields (`get_coin_or_default`), so the
    /// parallel path must accept the same transactions. This relaxes ONLY the
    /// "coin must exist in the database or be created earlier in the block"
    /// rejection of the post-hoc coin coherency verifier and the
    /// signature/predicate checks (mirroring how the node maps
    /// `utxo_validation` onto the sequential executor's `ExecutionOptions`);
    /// all cross-batch ordering/merge bookkeeping stays intact.
    ///
    /// When `true` (the production default) behavior is unchanged.
    pub utxo_validation: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worker_count: NonZeroUsize::new(1).expect("The value is not zero; qed"),
            worker_count_policy: WorkerCountPolicy::StaticMax,
            metrics: false,
            utxo_validation: true,
        }
    }
}
