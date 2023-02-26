use crate::service::state::test_builder::TestDataSource;

use super::*;

#[tokio::test]
async fn can_set_da_height() {
    let mut relayer = MockRelayerData::default();
    relayer.expect_wait_if_eth_syncing().returning(|| Ok(()));
    relayer.expect_update_synced().return_const(());
    relayer.expect_download_logs().returning(|_| Ok(()));
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_current: 300,
            eth_remote_finalization_period: 100,
            eth_local_finalized: None,
        },
    );
    run(&mut relayer).await.unwrap();
}

#[tokio::test]
async fn logs_are_downloaded_and_written() {
    let mut relayer = MockRelayerData::default();
    relayer.expect_wait_if_eth_syncing().returning(|| Ok(()));
    relayer.expect_update_synced().return_const(());
    relayer
        .expect_download_logs()
        .withf(|gap| gap.oldest() == 0 && gap.latest() == 200)
        .returning(|_| Ok(()));
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_current: 300,
            eth_remote_finalization_period: 100,
            eth_local_finalized: None,
        },
    );
    run(&mut relayer).await.unwrap();
}

mockall::mock! {
    RelayerData {}

    #[async_trait]
    impl EthRemote for RelayerData {
        async fn current(&self) -> anyhow::Result<u64>;
        fn finalization_period(&self) -> u64;
    }

    impl EthLocal for RelayerData {
        fn finalized(&self) -> Option<u64>;
    }

    #[async_trait]
    impl RelayerData for RelayerData{
        async fn wait_if_eth_syncing(&self) -> anyhow::Result<()>;

        async fn download_logs(
            &mut self,
            eth_sync_gap: &state::EthSyncGap,
        ) -> anyhow::Result<()>;

        fn update_synced(&self, state: &EthState);
    }
}

fn test_data_source(mock: &mut MockRelayerData, data: TestDataSource) {
    let out = data.eth_remote_current;
    mock.expect_current().returning(move || Ok(out));
    let out = data.eth_remote_finalization_period;
    mock.expect_finalization_period().returning(move || out);
    let out = data.eth_local_finalized;
    mock.expect_finalized().returning(move || out);
}
