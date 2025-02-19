#![allow(non_snake_case)]
#![allow(clippy::arithmetic_side_effects)]

use fuel_core_global_merkle_root_storage::ProcessedTransactions;
use fuel_core_services::Service as _;
use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
    },
    structured_storage::test::InMemoryStorage,
    transactional::{
        Modifiable,
        ReadTransaction,
    },
    Error as StorageError,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx::{
        Bytes32,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
};
use rand::{
    rngs::StdRng,
    SeedableRng as _,
};
use tokio::sync::{
    mpsc,
    watch,
};

use crate::{
    ports::BlockStream,
    service,
};

#[tokio::test]
async fn service__should_return_appropriate_status_when_started_and_stopped() {
    // Given
    let chain_id = ChainId::default();
    let storage = InMemoryStorage::default();
    let (_blocks_tx, blocks_rx) = mpsc::channel(128);
    let service = service::new_service(chain_id, storage, blocks_rx);

    // When
    let started_status = service.start_and_await().await.unwrap();
    let stopped_status = service.stop_and_await().await.unwrap();

    // Then
    assert_eq!(started_status, fuel_core_services::State::Started);
    assert_eq!(stopped_status, fuel_core_services::State::Stopped);
}

#[tokio::test]
async fn service__should_populate_storage_when_receiving_blocks() {
    let mut rng = StdRng::seed_from_u64(1337);

    // Given
    let chain_id = ChainId::default();
    let storage = SubscribableMemoryStorage::new();
    let mut storage_subscription = storage.subscribe();
    let (blocks_tx, blocks_rx) = mpsc::channel(128);
    let service = service::new_service(chain_id, storage, blocks_rx);

    let blocks = [
        random_block(&mut rng, 0u32, 0u64),
        random_block(&mut rng, 0u32, 1u64),
    ];
    let num_blocks = blocks.len();
    let last_transaction_id = blocks
        .last()
        .unwrap()
        .transactions()
        .last()
        .unwrap()
        .clone()
        .id(&chain_id);

    // When
    let started_status = service.start_and_await().await.unwrap();

    for block in blocks {
        blocks_tx.send(block).await.unwrap();
    }

    let storage_after_processing_blocks =
        storage_subscription.wait_for_nth_change(num_blocks).await;

    let stopped_status = service.stop_and_await().await.unwrap();

    let last_transaction_has_been_processed = storage_after_processing_blocks
        .read_transaction()
        .storage_as_ref::<ProcessedTransactions>()
        .get(&last_transaction_id)
        .unwrap()
        .is_some();

    // Then
    assert_eq!(started_status, fuel_core_services::State::Started);
    assert_eq!(stopped_status, fuel_core_services::State::Stopped);
    assert!(last_transaction_has_been_processed);
}

impl BlockStream for mpsc::Receiver<Block> {
    type Error = ClosedChannel;

    async fn next(&mut self) -> Result<Block, ClosedChannel> {
        self.recv().await.ok_or(ClosedChannel {})
    }
}

#[derive(Debug, Clone, derive_more::Display, derive_more::Error)]
pub struct ClosedChannel {}

fn random_block(
    rng: &mut StdRng,
    height: impl Into<BlockHeight>,
    da_height: impl Into<DaBlockHeight>,
) -> Block {
    let transactions = vec![random_transaction(rng), random_transaction(rng)];
    let header = PartialBlockHeader {
        consensus: ConsensusHeader {
            height: height.into(),
            ..Default::default()
        },
        application: ApplicationHeader {
            da_height: da_height.into(),
            ..Default::default()
        },
    };

    let outbox_message_ids = &[];
    let event_inbox_root = Bytes32::default();

    Block::new(
        header,
        transactions,
        outbox_message_ids,
        event_inbox_root,
        #[cfg(feature = "fault-proving")]
        &ChainId::default(),
    )
    .unwrap()
}

fn random_transaction(rng: &mut StdRng) -> Transaction {
    TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(0)
        .add_random_fee_input(rng)
        .finalize_as_transaction()
}

#[derive(Default)]
struct SubscribableMemoryStorage<Column> {
    inner: watch::Sender<Snapshot<InMemoryStorage<Column>>>,
}

impl<Column> SubscribableMemoryStorage<Column> {
    fn new() -> Self {
        let inner = watch::Sender::new(Snapshot::default());
        Self { inner }
    }

    fn subscribe(&self) -> Subscription<Column> {
        let inner = self.inner.subscribe();
        Subscription { inner }
    }
}

struct Subscription<Column> {
    inner: watch::Receiver<Snapshot<InMemoryStorage<Column>>>,
}

impl<Column: Clone> Subscription<Column> {
    async fn wait_for_nth_change(&mut self, n: usize) -> InMemoryStorage<Column> {
        self.inner
            .wait_for(|snapshot| snapshot.num_changes >= n)
            .await
            .unwrap()
            .inner
            .clone()
    }
}

#[derive(Debug, Default)]
struct Snapshot<Inner> {
    /// Number of times this snapshot has changed
    num_changes: usize,
    /// Latest version of the snapshot
    inner: Inner,
}

impl<Column> KeyValueInspect for SubscribableMemoryStorage<Column>
where
    Column: StorageColumn,
{
    type Column = Column;

    fn get(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> Result<Option<Value>, StorageError> {
        self.inner.borrow().inner.get(key, column)
    }
}

impl<Column> Modifiable for SubscribableMemoryStorage<Column>
where
    Column: StorageColumn,
{
    fn commit_changes(
        &mut self,
        changes: fuel_core_storage::transactional::Changes,
    ) -> fuel_core_storage::Result<()> {
        self.inner.send_modify(|snapshot| {
            snapshot.inner.commit_changes(changes).unwrap();
            snapshot.num_changes += 1;
        });

        Ok(())
    }
}
