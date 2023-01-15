//! Type safe state building

use super::*;
use async_trait::async_trait;

#[async_trait]
pub trait EthRemote {
    /// The current block height on the Ethereum node.
    async fn current(&self) -> anyhow::Result<u64>;
    /// The amount of blocks to wait before we consider an
    /// eth block finalized.
    fn finalization_period(&self) -> u64;
}

#[async_trait]
pub trait EthLocal {
    /// The current finalized eth block that the relayer has seen.
    fn finalized(&self) -> Option<u64>;
}

/// Build the Ethereum state.
pub async fn build_eth<T>(t: &T) -> anyhow::Result<EthState>
where
    T: EthRemote + EthLocal + ?Sized,
{
    Ok(EthState {
        remote: EthHeights::new(t.current().await?, t.finalization_period()),
        local: t.finalized(),
    })
}

#[cfg(test)]
pub mod test_builder {
    use super::*;
    #[derive(Debug, Default, Clone)]
    pub struct TestDataSource {
        pub eth_remote_current: u64,
        pub eth_remote_finalization_period: u64,
        pub eth_local_finalized: Option<u64>,
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

    impl EthLocal for TestDataSource {
        fn finalized(&self) -> Option<u64> {
            self.eth_local_finalized
        }
    }
}
