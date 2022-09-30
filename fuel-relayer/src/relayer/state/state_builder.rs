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

pub async fn build_eth<T>(t: &T) -> anyhow::Result<EthState>
where
    T: EthRemote + EthLocal + ?Sized,
{
    Ok(EthState {
        remote: EthHeights::new(t.current().await?, t.finalization_period()),
        local: t.finalized().await,
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
}
