#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Print execution backtraces if transaction execution reverts.
    pub backtrace: bool,
    /// Enables prometheus metrics for this fuel-service
    pub metrics: bool,
}
