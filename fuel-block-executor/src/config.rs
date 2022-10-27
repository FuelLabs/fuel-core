#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,

    /// Enabled prometheus metrics for this fuel-servive
    pub metrics: bool,
}
