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
        shared_state: SharedState {
            block_height_subscription_handler: Default::default(),
            block_subscription: tokio::sync::broadcast::channel(1).0,
        },
        startup_sync: None,
    }
}

struct FixedHeight(Option<BlockHeight>);

impl ports::worker::OnChainDatabase for FixedHeight {
    fn latest_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(self.0)
    }
}

struct CountingImporter {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl ports::worker::BlockImporter for CountingImporter {
    fn block_events(&self) -> BoxStream<SharedImportResult> {
        futures::stream::pending().into_boxed()
    }

    fn block_event_at_height(
        &self,
        height: BlockAt,
    ) -> anyhow::Result<SharedImportResult> {
        assert!(
            matches!(height, BlockAt::Genesis),
            "expected the genesis gap, got {height:?}"
        );
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Arc::new(
            ImportResult {
                sealed_block: Default::default(),
                tx_status: vec![],
                events: vec![],
                source: Default::default(),
            }
            .wrap(),
        ))
    }
}

#[tokio::test]
async fn into_task_returns_before_off_chain_catch_up() {
    use crate::database::database_description::off_chain::OffChain;

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let database = Database::<OffChain>::in_memory();
    let init = InitializeTask {
        chain_id: Default::default(),
        continue_on_error: false,
        tx_status_manager: MockTxStatusManager,
        blocks_events: futures::stream::pending().into_boxed(),
        block_importer: CountingImporter {
            calls: calls.clone(),
        },
        on_chain_database: FixedHeight(Some(BlockHeight::new(0))),
        off_chain_database: database.clone(),
        base_asset_id: Default::default(),
        shared_state: SharedState {
            block_height_subscription_handler: Default::default(),
            block_subscription: tokio::sync::broadcast::channel(1).0,
        },
    };

    let mut task = init
        .into_task(&StateWatcher::started(), ())
        .await
        .expect("into_task");
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "catch-up must not run inside into_task"
    );

    let mut watcher = StateWatcher::started();
    let pending =
        tokio::time::timeout(std::time::Duration::from_secs(2), task.run(&mut watcher))
            .await;
    assert!(
        pending.is_err(),
        "catch-up should finish and the live loop should stay pending, got {pending:?}"
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        ports::worker::OffChainDatabase::latest_height(&database).unwrap(),
        Some(BlockHeight::new(0))
    );
}

fn shared_import_at(height: u32) -> SharedImportResult {
    use fuel_core_types::blockchain::{
        SealedBlock,
        block::Block,
    };

    let mut block = Block::default();
    block
        .header_mut()
        .set_block_height(BlockHeight::new(height));
    Arc::new(
        ImportResult {
            sealed_block: SealedBlock {
                entity: block,
                consensus: Default::default(),
            },
            tx_status: vec![],
            events: vec![],
            source: Default::default(),
        }
        .wrap(),
    )
}

#[tokio::test]
async fn catch_up_applies_live_successor_without_revalidation() {
    use crate::database::database_description::off_chain::OffChain;
    use futures::StreamExt;

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let database = Database::<OffChain>::in_memory();
    // Genesis is not in the live stream. Height 1 was committed during catch-up
    // and must be applied from the stream, not re-validated.
    let blocks_events = futures::stream::iter(vec![shared_import_at(1)])
        .chain(futures::stream::pending())
        .into_boxed();
    let init = InitializeTask {
        chain_id: Default::default(),
        continue_on_error: false,
        tx_status_manager: MockTxStatusManager,
        blocks_events,
        block_importer: CountingImporter {
            calls: calls.clone(),
        },
        on_chain_database: FixedHeight(Some(BlockHeight::new(1))),
        off_chain_database: database.clone(),
        base_asset_id: Default::default(),
        shared_state: SharedState {
            block_height_subscription_handler: Default::default(),
            block_subscription: tokio::sync::broadcast::channel(1).0,
        },
    };

    let mut task = init
        .into_task(&StateWatcher::started(), ())
        .await
        .expect("into_task");
    let mut watcher = StateWatcher::started();
    let pending =
        tokio::time::timeout(std::time::Duration::from_secs(2), task.run(&mut watcher))
            .await;
    assert!(
        pending.is_err(),
        "catch-up should finish and the live loop should stay pending, got {pending:?}"
    );
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "only the historical genesis gap is re-validated"
    );
    assert_eq!(
        ports::worker::OffChainDatabase::latest_height(&database).unwrap(),
        Some(BlockHeight::new(1))
    );
}

#[tokio::test]
async fn catch_up_accepts_live_block_ahead_of_stale_on_chain_height() {
    use crate::database::database_description::off_chain::OffChain;
    use futures::StreamExt;

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let database = Database::<OffChain>::in_memory();
    // Height 1 is already committed on the stream. `latest_height` still
    // reports the previous tip. Catch-up must apply the live block, not
    // shut down with a rewind error.
    let blocks_events = futures::stream::iter(vec![shared_import_at(1)])
        .chain(futures::stream::pending())
        .into_boxed();
    let init = InitializeTask {
        chain_id: Default::default(),
        continue_on_error: false,
        tx_status_manager: MockTxStatusManager,
        blocks_events,
        block_importer: CountingImporter {
            calls: calls.clone(),
        },
        on_chain_database: FixedHeight(Some(BlockHeight::new(0))),
        off_chain_database: database.clone(),
        base_asset_id: Default::default(),
        shared_state: SharedState {
            block_height_subscription_handler: Default::default(),
            block_subscription: tokio::sync::broadcast::channel(1).0,
        },
    };

    let mut task = init
        .into_task(&StateWatcher::started(), ())
        .await
        .expect("into_task");
    let mut watcher = StateWatcher::started();
    let pending =
        tokio::time::timeout(std::time::Duration::from_secs(2), task.run(&mut watcher))
            .await;
    assert!(
        pending.is_err(),
        "catch-up should finish and the live loop should stay pending, got {pending:?}"
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        ports::worker::OffChainDatabase::latest_height(&database).unwrap(),
        Some(BlockHeight::new(1))
    );
}
