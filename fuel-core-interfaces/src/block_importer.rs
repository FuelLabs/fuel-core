// TODO full block
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    /// send this to eth
    NewBlockCreated(u64),
    NewBlockIncluded {
        height: u64,
        /// height where we are finalizing stake and token deposits.
        eth_height: u64,
    },
}
