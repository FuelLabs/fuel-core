use core::ops::RangeInclusive;
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
    remote: Option<FuelHeight>,
    local: FuelHeights,
}

type EthHeight = u64;
type FuelHeight = u32;

#[derive(Clone, Debug)]
struct Heights<T>(RangeInclusive<T>);

#[derive(Debug)]
struct EthHeights(Heights<u64>);
#[derive(Debug)]
struct FuelHeights(Heights<u32>);

#[derive(Clone, Debug)]
pub struct EthSyncGap(Heights<u64>);

impl SyncState {
    pub fn is_synced(&self) -> bool {
        self.eth.is_synced() && self.fuel.is_synced() && self.fuel.nothing_pending()
    }

    pub fn needs_to_sync_eth(&self) -> Option<EthSyncGap> {
        self.eth.needs_to_sync_eth()
    }

    pub fn needs_to_publish_fuel(&self) -> Option<u32> {
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
        self.local.finalized() >= self.local.current() || self.remote.is_some()
    }

    fn nothing_pending(&self) -> bool {
        self.remote.is_none()
    }

    pub fn needs_to_publish(&self) -> Option<u32> {
        (!self.is_synced()).then(|| self.local.current())
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

impl FuelHeights {
    fn new(current: u32, finalized: u32) -> Self {
        Self(Heights(finalized..=current))
    }
}

impl<T: Copy> Heights<T> {
    fn current(&self) -> T {
        *self.0.end()
    }

    fn finalized(&self) -> T {
        *self.0.start()
    }
}

impl Deref for EthHeights {
    type Target = Heights<u64>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for FuelHeights {
    type Target = Heights<u32>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
