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
pub trait FuelLocal {
    fn message_time_window(&self) -> Duration;
    fn min_messages_to_force_publish(&self) -> usize;
    async fn last_sent_time(&self) -> Option<Duration>;
    async fn latest_block_time(&self) -> Option<Duration>;
    async fn num_unpublished_messages(&self) -> usize;
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

pub async fn build_fuel<T>(t: &T) -> FuelState
where
    T: FuelLocal + ?Sized,
{
    let last = t.last_sent_time().await.unwrap_or_default();
    let latest = t.latest_block_time().await.unwrap_or(Duration::MAX);
    let duration_since_last_publish = latest.saturating_sub(last);
    FuelState {
        local: MessageState {
            times: MessageTimes::new(
                duration_since_last_publish,
                t.message_time_window(),
            ),
            num_unpublished: MessagesPending::new(
                t.num_unpublished_messages().await,
                t.min_messages_to_force_publish(),
            ),
        },
    }
}

pub async fn build<T>(t: &T) -> anyhow::Result<SyncState>
where
    T: EthRemote + EthLocal + FuelLocal + ?Sized,
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
        pub message_time_window: Duration,
        pub min_messages_to_force_publish: usize,
        pub last_sent_time: Option<Duration>,
        pub latest_block_time: Option<Duration>,
        pub num_unpublished_messages: usize,
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
    impl FuelLocal for TestDataSource {
        fn message_time_window(&self) -> Duration {
            self.message_time_window
        }

        fn min_messages_to_force_publish(&self) -> usize {
            self.min_messages_to_force_publish
        }

        async fn last_sent_time(&self) -> Option<Duration> {
            self.last_sent_time
        }

        async fn latest_block_time(&self) -> Option<Duration> {
            self.latest_block_time
        }

        async fn num_unpublished_messages(&self) -> usize {
            self.num_unpublished_messages
        }
    }
}
