#![allow(non_snake_case)]

use crate::{
    blocks::{
        BlockBytes,
        BlockSource,
    },
    db::{
        BlocksProvider,
        BlocksStorage,
    },
    result::Result,
    service::SharedState,
    task::Task,
};
use fuel_core_services::{
    RunnableTask,
    Service,
    State,
    StateWatcher,
    stream::BoxStream,
};
use fuel_core_types::fuel_types::BlockHeight;
use futures::{
    FutureExt,
    StreamExt,
};
use rand::{
    SeedableRng,
    prelude::StdRng,
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

type BlockRangeResponse = BoxStream<BlockBytes>;

struct FakeApi {}

#[async_trait::async_trait]
impl Service for FakeApi {
    fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn start_and_await(&self) -> anyhow::Result<State> {
        Ok(State::Started)
    }

    async fn await_start_or_stop(&self) -> anyhow::Result<State> {
        futures::future::pending().await
    }

    fn stop(&self) -> bool {
        false
    }

    async fn stop_and_await(&self) -> anyhow::Result<State> {
        Ok(State::Stopped)
    }

    async fn await_stop(&self) -> anyhow::Result<State> {
        futures::future::pending().await
    }

    fn state(&self) -> State {
        State::Started
    }

    fn state_watcher(&self) -> StateWatcher {
        StateWatcher::started()
    }
}

#[derive(Clone)]
struct FakeDB {
    map: Arc<Mutex<HashMap<BlockHeight, BlockBytes>>>,
}

impl FakeDB {
    fn new() -> Self {
        let map = Arc::new(Mutex::new(HashMap::new()));
        Self { map }
    }

    fn add_block(&mut self, height: BlockHeight, block: BlockBytes) {
        self.map.lock().unwrap().insert(height, block);
    }
}

impl BlocksStorage for FakeDB {
    type Block = BlockBytes;
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(&mut self, id: BlockHeight, block: &BlockBytes) -> Result<()> {
        self.map.lock().unwrap().insert(id, block.clone());
        Ok(())
    }
}

impl BlocksProvider for FakeDB {
    type Block = BlockBytes;
    type BlockRangeResponse = BlockRangeResponse;

    fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<BoxStream<BlockBytes>> {
        let mut blocks = vec![];
        let first: u32 = first.into();
        let last: u32 = last.into();
        for id in first..=last {
            if let Some(block) = self
                .map
                .lock()
                .expect("lets assume for now the test was written to avoid conflicts")
                .get(&id)
            {
                blocks.push(block.to_owned());
            }
        }
        Ok(Box::pin(futures::stream::iter(blocks)))
    }

    fn get_current_height(&self) -> Result<Option<BlockHeight>> {
        let map = self.map.lock().unwrap();
        let max_height = map.keys().max().cloned();
        Ok(max_height)
    }
}

struct FakeBlockSource {
    blocks: Vec<(BlockHeight, BlockBytes)>,
}

impl FakeBlockSource {
    fn new(blocks: Vec<(BlockHeight, BlockBytes)>) -> Self {
        Self { blocks }
    }
}

impl BlockSource for FakeBlockSource {
    type Block = BlockBytes;

    fn blocks_starting_from(
        &self,
        block_height: BlockHeight,
    ) -> impl Iterator<Item = Result<(BlockHeight, Self::Block)>> + Send + Sync + 'static
    {
        let start_height: u32 = block_height.into();
        self.blocks
            .clone()
            .into_iter()
            .filter(move |(height, _)| {
                let h: u32 = (*height).into();
                h >= start_height
            })
            .map(|(height, block)| Ok((height, block)))
    }
}

fn importer_stream(
    blocks: Vec<(BlockHeight, BlockBytes)>,
) -> BoxStream<anyhow::Result<(BlockHeight, BlockBytes)>> {
    let stream = futures::stream::iter(
        blocks
            .into_iter()
            .map(|(height, block)| Ok((height, block))),
    )
    .chain(futures::stream::once(futures::future::pending()));
    Box::pin(stream)
}

#[tokio::test]
async fn run__get_block_range__returns_expected_blocks() {
    let mut rng = StdRng::seed_from_u64(42);
    // Given
    let mut db = FakeDB::new();
    db.add_block(1.into(), BlockBytes::random(&mut rng));
    db.add_block(2.into(), BlockBytes::random(&mut rng));
    db.add_block(3.into(), BlockBytes::random(&mut rng));

    let shared_state = SharedState::new(db.clone(), 1_000);

    // When
    let result = shared_state.get_block_range(2, 3);

    // Then
    let stream = result.unwrap();
    let blocks = stream.collect::<Vec<BlockBytes>>().await;

    // TODO: Check values
    assert_eq!(blocks.len(), 2);
}

#[tokio::test]
async fn run__new_block_gets_added_to_db() {
    let mut rng = StdRng::seed_from_u64(42);

    // Given
    let db = FakeDB::new();
    let source = FakeBlockSource::new(vec![]);

    let sync_from = BlockHeight::from(123u32);
    let block = BlockBytes::random(&mut rng);
    let importer = importer_stream(vec![(sync_from, block.clone())]);
    let shared_state = SharedState::new(db.clone(), 1_000);
    let mut srv = Task::new(
        sync_from,
        Box::new(FakeApi {}),
        db,
        shared_state.clone(),
        source,
        importer,
    );
    let mut watcher = StateWatcher::started();

    // When
    // Import block event
    let _ = srv.run(&mut watcher).await;
    // Process event
    let _ = srv.run(&mut watcher).await;

    // Then

    let actual = shared_state
        .get_block_range(sync_from, sync_from)
        .unwrap()
        .next()
        .await
        .unwrap();
    assert_eq!(block, actual);
}

#[tokio::test]
async fn run__get_current_height__returns_expected_height() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let mut db = FakeDB::new();
    let expected_height = BlockHeight::from(3u32);
    db.add_block(1.into(), BlockBytes::random(&mut rng));
    db.add_block(2.into(), BlockBytes::random(&mut rng));
    db.add_block(expected_height, BlockBytes::random(&mut rng));

    let shared_state = SharedState::new(db.clone(), 1_000);

    // when
    let result = shared_state.get_current_height();

    // then
    let height = result.unwrap().unwrap();
    assert_eq!(expected_height, height);
}

#[tokio::test]
async fn run__new_block_subscription__sends_new_block() {
    let mut rng = StdRng::seed_from_u64(42);
    let db = FakeDB::new();
    let source = FakeBlockSource::new(vec![]);

    let sync_from = BlockHeight::from(123u32);
    let block = BlockBytes::random(&mut rng);
    let importer = importer_stream(vec![(sync_from, block.clone())]);
    let shared_state = SharedState::new(db.clone(), 1_000);
    let mut srv = Task::new(
        sync_from,
        Box::new(FakeApi {}),
        db,
        shared_state.clone(),
        source,
        importer,
    );
    let mut watcher = StateWatcher::started();

    // Given
    let mut subscription = shared_state.new_block_subscription();

    // When
    let _ = srv.run(&mut watcher).await;

    // Then
    let actual_block = subscription
        .next()
        .now_or_never()
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!((sync_from, Arc::new(block)), actual_block);
}

#[tokio::test]
async fn run__new_block_subscription__does_not_send_syncing_blocks() {
    let mut rng = StdRng::seed_from_u64(42);
    let db = FakeDB::new();

    let sync_from = BlockHeight::from(123u32);
    let block = BlockBytes::random(&mut rng);

    let source = FakeBlockSource::new(vec![(sync_from, block.clone())]);

    let importer = importer_stream(vec![]);
    let shared_state = SharedState::new(db.clone(), 1_000);
    let mut srv = Task::new(
        sync_from,
        Box::new(FakeApi {}),
        db,
        shared_state.clone(),
        source,
        importer,
    );
    let mut watcher = StateWatcher::started();

    // Given
    let mut subscription = shared_state.new_block_subscription();

    // When
    let _ = srv.run(&mut watcher).await;

    // Then
    let result = subscription.next().now_or_never();
    assert!(result.is_none());
}
