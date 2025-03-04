//! Clap configuration related to TxStatusManager service.

#[derive(Debug, Clone, clap::Args)]
pub struct TxStatusManagerArgs {
    /// The maximum number of active status subscriptions.
    #[clap(long = "tx-number-active-subscriptions", default_value = "4064", env)]
    pub tx_number_active_subscriptions: usize,
}
