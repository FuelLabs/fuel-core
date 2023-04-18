use clap::Args;

#[derive(Debug, Clone, Args)]
pub struct ProfilingArgs {
    #[clap(long = "pyroscope-url", env)]
    pub pyroscope_url: Option<String>,

    #[clap(long = "pprof-sample-rate", default_value = "100", env)]
    pub pprof_sample_rate: u32,
}
