use core::ops::RangeInclusive;
use std::ops::Deref;

pub struct SyncState {
    eth: EthState,
    fuel: FuelState,
}

pub struct EthState {
    remote: EthHeights,
    local: Height,
}

pub struct FuelState {
    local: FuelHeights,
}

type Height = u64;

pub struct Heights(RangeInclusive<u64>);

pub struct EthHeights(Heights);
pub struct FuelHeights(Heights);

// Type safe state building
pub struct EthRemote;
pub struct EthRemoteCurrent(u64);
pub struct EthRemoteFinalizationPeriod(u64);

pub struct EthRemoteHeights(EthHeights);

pub struct EthLocal;
pub struct EthLocalFinalized(u64);

pub struct FuelLocal;
pub struct FuelLocalCurrent(u64);
pub struct FuelLocalFinalized(u64);

impl EthRemote {
    pub fn current(height: u64) -> EthRemoteCurrent {
        EthRemoteCurrent(height)
    }
    pub fn finalization_period(p: u64) -> EthRemoteFinalizationPeriod {
        EthRemoteFinalizationPeriod(p)
    }
}

impl EthRemoteCurrent {
    pub fn finalization_period(self, p: u64) -> EthRemoteHeights {
        EthRemoteHeights(EthHeights::new(self.0, p))
    }
}

impl EthRemoteFinalizationPeriod {
    pub fn current(self, height: u64) -> EthRemoteHeights {
        EthRemoteHeights(EthHeights::new(height, self.0))
    }
}

impl EthLocal {
    pub fn finalized(f: u64) -> EthLocalFinalized {
        EthLocalFinalized(f)
    }
}

impl EthRemoteHeights {
    pub fn with_local(self, f: EthLocalFinalized) -> EthState {
        EthState {
            remote: self.0,
            local: f.0,
        }
    }
}

impl EthLocalFinalized {
    pub fn with_remote(self, remote: EthRemoteHeights) -> EthState {
        EthState {
            remote: remote.0,
            local: self.0,
        }
    }
}

impl FuelLocal {
    pub fn current(height: u64) -> FuelLocalCurrent {
        FuelLocalCurrent(height)
    }
    pub fn finalized(height: u64) -> FuelLocalFinalized {
        FuelLocalFinalized(height)
    }
}

impl FuelLocalCurrent {
    pub fn finalized(self, height: u64) -> FuelState {
        FuelState {
            local: FuelHeights::new(self.0, height),
        }
    }
}

impl FuelLocalFinalized {
    pub fn current(self, height: u64) -> FuelState {
        FuelState {
            local: FuelHeights::new(height, self.0),
        }
    }
}

impl FuelState {
    pub fn with_eth(self, eth: EthState) -> SyncState {
        SyncState { eth, fuel: self }
    }
}

impl EthState {
    pub fn with_fuel(self, fuel: FuelState) -> SyncState {
        SyncState { eth: self, fuel }
    }
}

impl EthHeights {
    fn new(current: u64, finalization_period: u64) -> Self {
        Self(Heights(
            current.saturating_sub(finalization_period)..=current,
        ))
    }
}

impl FuelHeights {
    fn new(current: u64, finalized: u64) -> Self {
        Self(Heights(finalized..=current))
    }
}

impl Heights {
    fn current(&self) -> u64 {
        *self.0.end()
    }

    fn finalized(&self) -> u64 {
        *self.0.start()
    }
}

impl Deref for EthHeights {
    type Target = Heights;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for FuelHeights {
    type Target = Heights;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
