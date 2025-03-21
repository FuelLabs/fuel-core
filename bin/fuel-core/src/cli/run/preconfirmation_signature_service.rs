#[derive(Debug, Clone, clap::Args)]
pub struct PreconfirmationArgs {
    /// The frequency at which we rotate the preconfirmation sub-key
    #[clap(
        long = "preconfirmation-key-rotation-frequency",
        env,
        default_value = "10m"
    )]
    pub key_rotation_interval: humantime::Duration,
    /// The frequency at which the preconfirmation sub-key expires
    #[clap(
        long = "preconfirmation-key-expiration-frequency",
        env,
        default_value = "20m"
    )]
    pub key_expiration_interval: humantime::Duration,
    /// The frequency at which the preconfirmation sub-key is echoed to the network
    #[clap(
        long = "preconfirmation-echo-delegation-frequency",
        env,
        default_value = "10s"
    )]
    pub echo_delegation_interval: humantime::Duration,
}
