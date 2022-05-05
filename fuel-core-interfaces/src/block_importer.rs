/// Currently just placeholder for new block included and new block created events.
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    /// send this to eth
    NewBlockCreated { height: u64 },
    NewBlockIncluded {
        height: u64,
        /// height where we are finalizing stake and token deposits.
        da_height: u64,
    },
}
