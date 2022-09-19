//! Type safe state building
use super::*;

pub struct EthRemote;
pub struct EthRemoteCurrent(u64);
pub struct EthRemoteFinalizationPeriod(u64);

pub struct EthRemoteHeights(EthHeights);

pub struct EthLocal;
pub struct EthLocalFinalized(u64);

pub struct FuelLocal;
pub struct FuelLocalCurrent(u32);
pub struct FuelLocalFinalized(u32);

pub struct FuelLocalHeights(FuelHeights);

pub struct FuelRemote;

pub struct FuelRemotePending(Option<u32>);

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
    pub fn current(height: u32) -> FuelLocalCurrent {
        FuelLocalCurrent(height)
    }
    pub fn finalized(height: u32) -> FuelLocalFinalized {
        FuelLocalFinalized(height)
    }
}

impl FuelLocalCurrent {
    pub fn finalized(self, height: u32) -> FuelLocalHeights {
        FuelLocalHeights(FuelHeights::new(self.0, height))
    }
}

impl FuelLocalFinalized {
    pub fn current(self, height: u32) -> FuelLocalHeights {
        FuelLocalHeights(FuelHeights::new(height, self.0))
    }
}

impl FuelRemote {
    pub fn pending(height: Option<u32>) -> FuelRemotePending {
        FuelRemotePending(height)
    }
}

impl FuelRemotePending {
    pub fn with_local(self, local: FuelLocalHeights) -> FuelState {
        FuelState {
            remote: self.0,
            local: local.0,
        }
    }
}

impl FuelLocalHeights {
    pub fn with_remote(self, remote: FuelRemotePending) -> FuelState {
        FuelState {
            remote: remote.0,
            local: self.0,
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
