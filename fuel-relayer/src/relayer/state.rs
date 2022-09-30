use core::ops::RangeInclusive;
pub use state_builder::*;
use std::ops::Deref;

mod state_builder;

#[cfg(test)]
mod test;

#[derive(Debug)]
pub struct EthState {
    remote: EthHeights,
    local: EthHeight,
}

type EthHeight = u64;

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
pub struct EthSyncPage {
    current: RangeInclusive<u64>,
    size: u64,
    end: u64,
}

impl EthState {
    pub fn is_synced(&self) -> bool {
        self.local >= self.remote.finalized()
    }
    pub fn needs_to_sync_eth(&self) -> Option<EthSyncGap> {
        (!self.is_synced()).then(|| EthSyncGap::new(self.local, self.remote.finalized()))
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
    pub(crate) fn new(local: u64, remote: u64) -> Self {
        Self(Heights(local..=remote))
    }

    pub fn oldest(&self) -> u64 {
        *self.0 .0.start()
    }

    pub fn latest(&self) -> u64 {
        *self.0 .0.end()
    }

    pub fn page(&self, page_size: u64) -> EthSyncPage {
        EthSyncPage {
            current: self.oldest()
                ..=self
                    .oldest()
                    .saturating_add(page_size.saturating_sub(1))
                    .min(self.latest()),
            size: page_size,
            end: self.latest(),
        }
    }
}

impl EthSyncPage {
    pub fn reduce(&mut self) {
        dbg!(&self.current);
        self.current = self.current.start().saturating_add(self.size)
            ..=self.current.end().saturating_add(self.size).min(self.end);
        dbg!(&self.current);
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_empty() || self.size == 0
    }

    pub fn oldest(&self) -> u64 {
        *self.current.start()
    }

    pub fn latest(&self) -> u64 {
        *self.current.end()
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
