#![allow(non_snake_case)]

use super::*;
use crate::{
    database::Database,
    graphql_api::storage::relayed_transactions::RelayedTransactionStatuses,
};
use fuel_core_services::stream::IntoBoxStream;
use fuel_core_storage::StorageAsRef;
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::BlockHeight,
    services::transaction_status::TransactionStatus,
};
use std::sync::Arc;

struct MockTxStatusManager;

impl ports::worker::TxStatusCompletion for MockTxStatusManager {
    fn send_complete(
        &self,
        _id: Bytes32,
        _block_height: &BlockHeight,
        _status: TransactionStatus,
    ) {
        // Do nothing
    }
}

#[tokio::test]
async fn run__relayed_transaction_events_are_added_to_storage() {
    let tx_id: Bytes32 = [1; 32].into();
    let block_height = 8.into();
    let failure = "blah blah blah".to_string();
    let database = Database::in_memory();
    let mut state_watcher = StateWatcher::started();

    // given
    let event = Event::ForcedTransactionFailed {
        id: tx_id.into(),
        block_height,
        failure: failure.clone(),
    };
    let block_importer = block_importer_for_event(event);

    // when
    let mut task =
        worker_task_with_block_importer_and_db(block_importer, database.clone());
    let _ = task.run(&mut state_watcher).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // then
    let expected = RelayedTransactionStatus::Failed {
        block_height,
        failure,
    };
    let storage = database.storage_as_ref::<RelayedTransactionStatuses>();
    let actual = storage.get(&tx_id).unwrap().unwrap();
    assert_eq!(*actual, expected);
}

fn block_importer_for_event(event: Event) -> BoxStream<SharedImportResult> {
    let block = Arc::new(
        ImportResult {
            sealed_block: Default::default(),
            tx_status: vec![],
            events: vec![event],
            source: Default::default(),
        }
        .wrap(),
    );
    let blocks: Vec<SharedImportResult> = vec![block];
    tokio_stream::iter(blocks).into_boxed()
}

fn worker_task_with_block_importer_and_db<D: ports::worker::OffChainDatabase>(
    block_importer: BoxStream<SharedImportResult>,
    database: D,
) -> Task<MockTxStatusManager, D> {
    let tx_status_manager = MockTxStatusManager;
    let chain_id = Default::default();
    Task {
        tx_status_manager,
        block_importer,
        database,
        chain_id,
        continue_on_error: false,
        balances_indexation_enabled: true,
        coins_to_spend_indexation_enabled: true,
        asset_metadata_indexation_enabled: true,
        base_asset_id: Default::default(),
        block_height_subscription_handler: Default::default(),
    }
}
