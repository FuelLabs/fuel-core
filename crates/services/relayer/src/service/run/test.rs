use crate::service::state::test_builder::TestDataSource;
use std::sync::{
    Arc,
    Mutex,
};

use super::*;

#[tokio::test]
async fn can_set_da_height() {
    let mut relayer = MockRelayerData::default();
    relayer.expect_wait_if_eth_syncing().returning(|| Ok(()));
    relayer.expect_update_synced().return_const(());
    relayer.expect_download_logs().returning(|_| Ok(()));
    relayer.expect_storage_da_block_height().returning(|| None);
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_finalized: 200,
            eth_local_finalized: 0,
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
        .withf(|gap| gap.oldest() == 1 && gap.latest() == 200)
        .returning(|_| Ok(()));
    relayer.expect_storage_da_block_height().returning(|| None);
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_finalized: 200,
            eth_local_finalized: 0,
        },
    );
    run(&mut relayer).await.unwrap();
}

#[tokio::test]
async fn logs_are_downloaded_and_written_partially() {
    let mut relayer = MockRelayerData::default();
    let eth_state = Arc::new(Mutex::new(EthState::new(200, 0)));
    relayer.expect_wait_if_eth_syncing().returning(|| Ok(()));
    relayer.expect_update_synced().returning({
        let eth_state = eth_state.clone();
        move |state| {
            let mut eth_state = eth_state.lock().unwrap();
            *eth_state = *state;
        }
    });
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_finalized: 200,
            eth_local_finalized: 0,
        },
    );

    // Given
    relayer
        .expect_download_logs()
        .withf(|gap| gap.oldest() == 1 && gap.latest() == 200)
        .returning(|_| Ok(()));
    relayer
        .expect_storage_da_block_height()
        .returning(|| Some(100));

    // When
    let result = run(&mut relayer).await;

    // Then
    assert!(result.is_ok());
    let actual = *eth_state.lock().unwrap();
    let expected = EthState::new(200, 100);

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn logs_are_downloaded_and_written_failed_state_is_not_updated() {
    let mut relayer = MockRelayerData::default();
    let eth_state = Arc::new(Mutex::new(EthState::new(200, 0)));
    relayer.expect_wait_if_eth_syncing().returning(|| Ok(()));
    relayer.expect_update_synced().returning({
        let eth_state = eth_state.clone();
        move |state| {
            let mut eth_state = eth_state.lock().unwrap();
            *eth_state = *state;
        }
    });
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_finalized: 200,
            eth_local_finalized: 0,
        },
    );

    // Given
    relayer
        .expect_download_logs()
        .withf(|gap| gap.oldest() == 1 && gap.latest() == 200)
        .returning(|_| Err(anyhow::anyhow!("failed to download logs")));
    relayer.expect_storage_da_block_height().returning(|| None);

    // When
    let result = run(&mut relayer).await;

    // Then
    assert!(result.is_err());
    let actual = *eth_state.lock().unwrap();
    let expected = EthState::new(200, 0);

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn logs_are_downloaded_and_written_failed_but_state_is_updated_because_of_storage_update(
) {
    let mut relayer = MockRelayerData::default();
    let eth_state = Arc::new(Mutex::new(EthState::new(200, 0)));
    relayer.expect_wait_if_eth_syncing().returning(|| Ok(()));
    relayer.expect_update_synced().returning({
        let eth_state = eth_state.clone();
        move |state| {
            let mut eth_state = eth_state.lock().unwrap();
            *eth_state = *state;
        }
    });
    test_data_source(
        &mut relayer,
        TestDataSource {
            eth_remote_finalized: 200,
            eth_local_finalized: 0,
        },
    );

    // Given
    relayer
        .expect_download_logs()
        .withf(|gap| gap.oldest() == 1 && gap.latest() == 200)
        .returning(|_| Err(anyhow::anyhow!("failed to download logs")));
    relayer
        .expect_storage_da_block_height()
        .returning(|| Some(100));

    // When
    let result = run(&mut relayer).await;

    // Then
    assert!(result.is_err());
    let actual = *eth_state.lock().unwrap();
    let expected = EthState::new(200, 100);

    assert_eq!(actual, expected);
}

mockall::mock! {
    RelayerData {}

    impl EthRemote for RelayerData {
        async fn finalized(&self) -> anyhow::Result<u64>;
    }

    impl EthLocal for RelayerData {
        fn observed(&self) -> u64;
    }

    impl RelayerData for RelayerData{
        async fn wait_if_eth_syncing(&self) -> anyhow::Result<()>;

        async fn download_logs(
            &mut self,
            eth_sync_gap: &state::EthSyncGap,
        ) -> anyhow::Result<()>;

        fn update_synced(&self, state: &EthState);

        fn storage_da_block_height(&self) -> Option<u64>;
    }
}

fn test_data_source(mock: &mut MockRelayerData, data: TestDataSource) {
    let out = data.eth_remote_finalized;
    mock.expect_finalized().returning(move || Ok(out));
    let out = data.eth_local_finalized;
    mock.expect_observed().returning(move || out);
}
