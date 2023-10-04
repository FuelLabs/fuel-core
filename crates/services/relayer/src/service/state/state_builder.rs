//! Type safe state building

use super::*;
use async_trait::async_trait;

#[async_trait]
pub trait EthRemote {
    /// The most recently finalized height on the Ethereum node.
    async fn finalized(&self) -> anyhow::Result<u64>;
}

#[async_trait]
pub trait EthLocal {
    /// The current finalized eth block that the relayer has seen.
    fn observed(&self) -> Option<u64>;
}

/// Build the Ethereum state.
pub async fn build_eth<T>(t: &T) -> anyhow::Result<EthState>
where
    T: EthRemote + EthLocal + ?Sized,
{
    let finalized = t.finalized().await?;
    let observed = t.observed();
    let eth_state = EthState {
        remote: finalized,
        local: observed,
    };
    Ok(eth_state)
}

#[cfg(test)]
pub mod test_builder {
    use super::*;
    #[derive(Debug, Default, Clone)]
    pub struct TestDataSource {
        pub eth_remote_finalized: u64,
        pub eth_local_finalized: Option<u64>,
    }

    #[async_trait]
    impl EthRemote for TestDataSource {
        async fn finalized(&self) -> anyhow::Result<u64> {
            Ok(self.eth_remote_finalized)
        }
    }

    impl EthLocal for TestDataSource {
        fn observed(&self) -> Option<u64> {
            self.eth_local_finalized
        }
    }
}
