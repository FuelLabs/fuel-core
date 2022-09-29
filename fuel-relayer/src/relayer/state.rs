use core::{
    ops::RangeInclusive,
    time::Duration,
};
pub use state_builder::*;
use std::ops::Deref;

mod state_builder;

#[cfg(test)]
mod test;

#[derive(Debug)]
pub struct SyncState {
    eth: EthState,
    fuel: FuelState,
}

#[derive(Debug)]
pub struct EthState {
    remote: EthHeights,
    local: EthHeight,
}

#[derive(Debug)]
pub struct FuelState {
    local: MessageState,
}

type EthHeight = u64;

#[derive(Debug, Clone)]
pub struct MessageState {
    times: MessageTimes,
    num_unpublished: MessagesPending,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageRoot;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageOrder {
    pub block_number: u32,
    pub transaction_index: u16,
    pub output_index: u8,
}

#[derive(Clone, Debug)]
struct Heights<T>(RangeInclusive<T>);

#[derive(Debug)]
struct EthHeights(Heights<u64>);

#[derive(Clone, Debug)]
pub struct EthSyncGap(Heights<u64>);

#[derive(Clone, Debug)]
pub struct MessageTimes(Heights<Duration>);

#[derive(Clone, Debug)]
pub struct MessagesPending(Heights<usize>);

impl SyncState {
    pub fn is_synced(&self) -> bool {
        self.eth.is_synced() && self.fuel.is_synced()
    }

    pub fn needs_to_sync_eth(&self) -> Option<EthSyncGap> {
        self.eth.needs_to_sync_eth()
    }

    pub fn needs_to_publish_fuel(&self) -> bool {
        self.fuel.needs_to_publish()
    }
}

impl EthState {
    fn is_synced(&self) -> bool {
        self.local >= self.remote.finalized()
    }
    pub fn needs_to_sync_eth(&self) -> Option<EthSyncGap> {
        (!self.is_synced()).then(|| EthSyncGap::new(self.local, self.remote.finalized()))
    }
}

impl FuelState {
    fn is_synced(&self) -> bool {
        !self.local.due_for_publish()
    }

    pub fn needs_to_publish(&self) -> bool {
        self.local.due_for_publish()
    }
}

impl EthHeights {
    fn new(current: u64, finalization_period: u64) -> Self {
        Self(Heights(
            current.saturating_sub(finalization_period)..=current,
        ))
    }
}

impl EthSyncGap {
    fn new(local: u64, remote: u64) -> Self {
        Self(Heights(local..=remote))
    }

    pub fn oldest(&self) -> u64 {
        *self.0 .0.start()
    }

    pub fn latest(&self) -> u64 {
        *self.0 .0.end()
    }
}
impl MessageState {
    fn due_for_publish(&self) -> bool {
        *self.num_unpublished.start() > 0
            && (self.num_unpublished.is_empty() || self.times.is_empty())
    }
}

impl MessageTimes {
    fn new(duration_since_last_sent: Duration, interval: Duration) -> Self {
        Self(Heights(duration_since_last_sent..=interval))
    }
}

impl MessagesPending {
    fn new(num_unpublished: usize, min_to_force_publish: usize) -> Self {
        Self(Heights(num_unpublished..=min_to_force_publish))
    }
}

impl<T: Clone> Heights<T> {
    fn finalized(&self) -> T {
        self.0.start().clone()
    }
}

impl Deref for EthHeights {
    type Target = Heights<u64>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for MessageTimes {
    type Target = RangeInclusive<Duration>;

    fn deref(&self) -> &Self::Target {
        &self.0 .0
    }
}

impl Deref for MessagesPending {
    type Target = RangeInclusive<usize>;

    fn deref(&self) -> &Self::Target {
        &self.0 .0
    }
}
