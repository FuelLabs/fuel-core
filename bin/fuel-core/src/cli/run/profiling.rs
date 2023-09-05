use clap::Args;

#[derive(Debug, Clone, Args)]
pub struct ProfilingArgs {
    /// Enables realtime profiling with pyroscope if set, and streams results to the pyroscope endpoint.
    /// For best results, the binary should be built with debug symbols included.
    #[clap(long = "pyroscope-url", env)]
    pub pyroscope_url: Option<String>,

    /// Pyroscope sample frequency in hertz. A higher sample rate improves profiling granularity
    /// at the cost of additional measurement overhead.
    #[clap(long = "pprof-sample-rate", default_value = "100", env)]
    pub pprof_sample_rate: u32,
}
