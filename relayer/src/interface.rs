


pub trait BlockRelayer {
    /// new round and new leader announcment
    fn sub_new_block();

    /// only for unusual cases where block is not good
    fn challenge_block();

    /// publish block if it is our time to produce it
    fn publish_block();
}

/// Happenings inside lagged time frame (15blocks) 
/// so that in the way it deminish potential mainnet reorgs. Still need to handle bigger ones.
/// 
/// In essence reorg happens
/// 
/// Needs to be atomic so change of deposit and withdrawal are only valid after whole block
/// execution. If it happen to be in middle, just ignore it. Changes can be atomic between two
/// proposed blocks
/// 
/// Should relayer be responsible for lagged time frame, or should we delay that to consensus?
pub trait ValidatorRelayer {
    fn sub_deposit();
    fn sub_withdrawal();
}


pub trait BridgeRelayer {
    fn sub_deposited_assert();
    fn withdrwal_assert();
}


