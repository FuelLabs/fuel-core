use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "chaos-test",
    about = "Chaos test for HA leader lock failover"
)]
pub struct Cli {
    /// RNG seed for reproducibility (random if omitted)
    #[arg(long)]
    pub seed: Option<u64>,

    /// Total test duration (e.g., "5m", "30s")
    #[arg(long, default_value = "5m")]
    pub duration: humantime::Duration,

    /// Number of PoA producer nodes
    #[arg(long, default_value = "3")]
    pub nodes: usize,

    /// Number of Redis nodes
    #[arg(long, default_value = "3")]
    pub redis_nodes: usize,

    /// Block production interval
    #[arg(long, default_value = "200ms")]
    pub block_time: humantime::Duration,

    /// Average interval between fault injections
    #[arg(long, default_value = "2s")]
    pub fault_interval: humantime::Duration,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
}
