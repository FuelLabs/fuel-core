#![allow(non_snake_case)]

use super::*;
use crate::{
    database::Database,
    graphql_api::storage::relayed_transactions::RelayedTransactionStatuses,
};
use fuel_core_services::{
    stream::IntoBoxStream,
    State,
};
use fuel_core_storage::StorageAsRef;
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::BlockHeight,
    services::txpool::TransactionStatus,
    tai64::Tai64,
};
use std::sync::Arc;

struct MockTxPool;

impl ports::worker::TxPool for MockTxPool {
    fn send_complete(
        &self,
        _id: Bytes32,
        _block_height: &BlockHeight,
        _status: TransactionStatus,
    ) {
        // Do nothing
        ()
    }
}

#[tokio::test]
async fn run__relayed_transaction_events_are_added_to_storage() {
    let tx_id: Bytes32 = [1; 32].into();
    let block_height = 8.into();
    let block_time = Tai64::UNIX_EPOCH;
    let failure = "peanut butter chocolate cake with Kool-Aid".to_string();
    let event = Event::ForcedTransactionFailed {
        id: tx_id.into(),
        block_height,
        block_time: Tai64::UNIX_EPOCH,
        failure: failure.clone(),
    };
    let tx_pool = MockTxPool;
    let block = Arc::new(ImportResult {
        sealed_block: Default::default(),
        tx_status: vec![],
        events: vec![event],
        source: Default::default(),
    });
    let blocks: Vec<Arc<dyn Deref<Target = ImportResult> + Send + Sync>> = vec![block];
    let block_importer = tokio_stream::iter(blocks).into_boxed();
    let database = Database::in_memory();
    let chain_id = Default::default();
    let mut task = Task {
        tx_pool,
        block_importer,
        database: database.clone(),
        chain_id,
    };
    // given

    let expected = RelayedTransactionStatus::Failed {
        block_height,
        block_time,
        failure,
    };

    // when
    let (_sender, receiver) = tokio::sync::watch::channel(State::Started);
    let mut state_watcher = receiver.into();
    task.run(&mut state_watcher).await.unwrap();

    // then
    let storage = database.storage_as_ref::<RelayedTransactionStatuses>();
    let actual = storage.get(&tx_id).unwrap().unwrap();
    assert_eq!(*actual, expected);
}
