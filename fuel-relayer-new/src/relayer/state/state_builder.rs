//! Type safe state building
use super::*;
use async_trait::async_trait;

#[async_trait]
pub trait EthRemote {
    async fn current(&self) -> anyhow::Result<u64>;
    fn finalization_period(&self) -> u64;
}

#[async_trait]
pub trait EthLocal {
    async fn finalized(&self) -> u64;
}

#[async_trait]
pub trait FuelRemote {
    async fn pending(&self) -> Option<u32>;
}

#[async_trait]
pub trait FuelLocal {
    async fn current(&self) -> u32;
    async fn finalized(&self) -> u32;
}

pub async fn build_eth<T>(t: &T) -> anyhow::Result<EthState>
where
    T: EthRemote + EthLocal,
{
    Ok(EthState {
        remote: EthHeights::new(t.current().await?, t.finalization_period()),
        local: t.finalized().await,
    })
}

pub async fn build_fuel<T>(t: &T) -> FuelState
where
    T: FuelRemote + FuelLocal,
{
    FuelState {
        local: FuelHeights::new(t.current().await, t.finalized().await),
        remote: t.pending().await,
    }
}

pub async fn build<T>(t: &T) -> anyhow::Result<SyncState>
where
    T: EthRemote + EthLocal + FuelLocal + FuelRemote,
{
    Ok(SyncState {
        eth: build_eth(t).await?,
        fuel: build_fuel(t).await,
    })
}

#[cfg(test)]
pub mod test_builder {
    use super::*;
    #[derive(Debug, Default, Clone)]
    pub struct TestDataSource {
        pub eth_remote_current: u64,
        pub eth_remote_finalization_period: u64,
        pub eth_local_finalized: u64,
        pub fuel_local_current: u32,
        pub fuel_local_finalized: u32,
        pub fuel_remote_pending: Option<u32>,
    }

    #[async_trait]
    impl EthRemote for TestDataSource {
        async fn current(&self) -> anyhow::Result<u64> {
            Ok(self.eth_remote_current)
        }
        fn finalization_period(&self) -> u64 {
            self.eth_remote_finalization_period
        }
    }

    #[async_trait]
    impl EthLocal for TestDataSource {
        async fn finalized(&self) -> u64 {
            self.eth_local_finalized
        }
    }

    #[async_trait]
    impl FuelRemote for TestDataSource {
        async fn pending(&self) -> Option<u32> {
            self.fuel_remote_pending
        }
    }

    #[async_trait]
    impl FuelLocal for TestDataSource {
        async fn current(&self) -> u32 {
            self.fuel_local_current
        }

        async fn finalized(&self) -> u32 {
            self.fuel_local_finalized
        }
    }
}

// pub struct EthRemote;
// pub struct EthRemoteCurrent(u64);
// pub struct EthRemoteFinalizationPeriod(u64);

// pub struct EthRemoteHeights(EthHeights);

// pub struct EthLocal;
// pub struct EthLocalFinalized(u64);

// pub struct FuelLocal;
// pub struct FuelLocalCurrent(u32);
// pub struct FuelLocalFinalized(u32);

// pub struct FuelLocalHeights(FuelHeights);

// pub struct FuelRemote;

// pub struct FuelRemotePending(Option<u32>);

// impl EthRemote {
//     pub fn current(height: u64) -> EthRemoteCurrent {
//         EthRemoteCurrent(height)
//     }
//     pub fn finalization_period(p: u64) -> EthRemoteFinalizationPeriod {
//         EthRemoteFinalizationPeriod(p)
//     }
// }

// impl EthRemoteCurrent {
//     pub fn finalization_period(self, p: u64) -> EthRemoteHeights {
//         EthRemoteHeights(EthHeights::new(self.0, p))
//     }
// }

// impl EthRemoteFinalizationPeriod {
//     pub fn current(self, height: u64) -> EthRemoteHeights {
//         EthRemoteHeights(EthHeights::new(height, self.0))
//     }
// }

// impl EthLocal {
//     pub fn finalized(f: u64) -> EthLocalFinalized {
//         EthLocalFinalized(f)
//     }
// }

// impl EthRemoteHeights {
//     pub fn with_local(self, f: EthLocalFinalized) -> EthState {
//         EthState {
//             remote: self.0,
//             local: f.0,
//         }
//     }
// }

// impl EthLocalFinalized {
//     pub fn with_remote(self, remote: EthRemoteHeights) -> EthState {
//         EthState {
//             remote: remote.0,
//             local: self.0,
//         }
//     }
// }

// impl FuelLocal {
//     pub fn current(height: u32) -> FuelLocalCurrent {
//         FuelLocalCurrent(height)
//     }
//     pub fn finalized(height: u32) -> FuelLocalFinalized {
//         FuelLocalFinalized(height)
//     }
// }

// impl FuelLocalCurrent {
//     pub fn finalized(self, height: u32) -> FuelLocalHeights {
//         FuelLocalHeights(FuelHeights::new(self.0, height))
//     }
// }

// impl FuelLocalFinalized {
//     pub fn current(self, height: u32) -> FuelLocalHeights {
//         FuelLocalHeights(FuelHeights::new(height, self.0))
//     }
// }

// impl FuelRemote {
//     pub fn pending(height: Option<u32>) -> FuelRemotePending {
//         FuelRemotePending(height)
//     }
// }

// impl FuelRemotePending {
//     pub fn with_local(self, local: FuelLocalHeights) -> FuelState {
//         FuelState {
//             remote: self.0,
//             local: local.0,
//         }
//     }
// }

// impl FuelLocalHeights {
//     pub fn with_remote(self, remote: FuelRemotePending) -> FuelState {
//         FuelState {
//             remote: remote.0,
//             local: self.0,
//         }
//     }
// }

// impl FuelState {
//     pub fn with_eth(self, eth: EthState) -> SyncState {
//         SyncState { eth, fuel: self }
//     }
// }

// impl EthState {
//     pub fn with_fuel(self, fuel: FuelState) -> SyncState {
//         SyncState { eth: self, fuel }
//     }
// }
