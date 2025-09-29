#![allow(non_snake_case)]

use super::*;
use crate::{
    api::BlockAggregatorQuery,
    blocks::{
        Block,
        BlockSourceEvent,
    },
    result::{
        Error,
        Result,
    },
};
use anyhow::anyhow;
use fuel_core_services::stream::BoxStream;
use futures::StreamExt;
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
use tokio::{
    sync::mpsc::{
        Receiver,
        Sender,
    },
    time::error::Elapsed,
};

type BlockRangeResponse = BoxStream<Block>;

struct FakeApi<T> {
    receiver: Receiver<BlockAggregatorQuery<T>>,
}

impl<T> FakeApi<T> {
    fn new() -> (Self, Sender<BlockAggregatorQuery<T>>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let api = Self { receiver };
        (api, sender)
    }
}

impl<T: Send> BlockAggregatorApi for FakeApi<T> {
    type BlockRangeResponse = T;
    async fn await_query(&mut self) -> Result<BlockAggregatorQuery<T>> {
        Ok(self.receiver.recv().await.unwrap())
    }
}

struct FakeDB {
    map: Arc<Mutex<HashMap<BlockHeight, Block>>>,
}

impl FakeDB {
    fn new() -> Self {
        let map = Arc::new(Mutex::new(HashMap::new()));
        Self { map }
    }

    fn add_block(&mut self, height: BlockHeight, block: Block) {
        self.map.lock().unwrap().insert(height, block);
    }

    fn clone_inner(&self) -> Arc<Mutex<HashMap<BlockHeight, Block>>> {
        self.map.clone()
    }
}

impl BlockAggregatorDB for FakeDB {
    type BlockRangeResponse = BlockRangeResponse;

    async fn store_block(&mut self, id: BlockHeight, block: Block) -> Result<()> {
        self.map.lock().unwrap().insert(id, block);
        Ok(())
    }

    async fn get_block_range(
        &self,
        first: BlockHeight,
        last: BlockHeight,
    ) -> Result<BoxStream<Block>> {
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

    async fn get_current_height(&self) -> Result<BlockHeight> {
        let map = self.map.lock().unwrap();
        let max_height = map.keys().max().cloned().unwrap_or(BlockHeight::from(0u32));
        Ok(max_height)
    }
}

struct FakeBlockSource {
    blocks: Receiver<BlockSourceEvent>,
}

impl FakeBlockSource {
    fn new() -> (Self, Sender<BlockSourceEvent>) {
        let (_sender, receiver) = tokio::sync::mpsc::channel(1);
        let _self = Self { blocks: receiver };
        (_self, _sender)
    }
}

impl BlockSource for FakeBlockSource {
    async fn next_block(&mut self) -> Result<BlockSourceEvent> {
        self.blocks
            .recv()
            .await
            .ok_or(Error::BlockSource(anyhow!("Channel closed")))
    }

    async fn drain(&mut self) -> Result<()> {
        todo!()
    }
}

#[tokio::test]
async fn run__get_block_range__returns_expected_blocks() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, sender) = FakeApi::new();
    let mut db = FakeDB::new();
    db.add_block(1.into(), Block::random(&mut rng));
    db.add_block(2.into(), Block::random(&mut rng));
    db.add_block(3.into(), Block::random(&mut rng));

    let (source, _block_sender) = FakeBlockSource::new();

    let mut srv = BlockAggregator::new(api, db, source);
    let mut watcher = StateWatcher::started();
    let (query, response) = BlockAggregatorQuery::get_block_range(2, 3);

    // when
    sender.send(query).await.unwrap();
    let _ = srv.run(&mut watcher).await;

    // then
    let stream = response.await.unwrap();
    let blocks = stream.collect::<Vec<Block>>().await;

    // TODO: Check values
    assert_eq!(blocks.len(), 2);

    // cleanup
    drop(_block_sender);
}

#[tokio::test]
async fn run__new_block_gets_added_to_db() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, _sender) = FakeApi::new();
    let db = FakeDB::new();
    let db_map = db.clone_inner();
    let (source, source_sender) = FakeBlockSource::new();
    let mut srv = BlockAggregator::new(api, db, source);

    let block = Block::random(&mut rng);
    let id = BlockHeight::from(123u32);
    let mut watcher = StateWatcher::started();

    // when
    let event = BlockSourceEvent::NewBlock(id, block.clone());
    source_sender.send(event).await.unwrap();
    let _ = srv.run(&mut watcher).await;

    // then
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let actual = db_map.lock().unwrap().get(&id).unwrap().clone();
    assert_eq!(block, actual);
}

#[tokio::test]
async fn run__get_current_height__returns_expected_height() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, sender) = FakeApi::new();
    let mut db = FakeDB::new();
    let expected_height = BlockHeight::from(3u32);
    db.add_block(1.into(), Block::random(&mut rng));
    db.add_block(2.into(), Block::random(&mut rng));
    db.add_block(expected_height, Block::random(&mut rng));

    let (source, _block_sender) = FakeBlockSource::new();
    let mut srv = BlockAggregator::new(api, db, source);

    let mut watcher = StateWatcher::started();
    let (query, response) = BlockAggregatorQuery::get_current_height();

    // when
    sender.send(query).await.unwrap();
    let _ = srv.run(&mut watcher).await;

    // then
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let height = response.await.unwrap();
    assert_eq!(expected_height, height);

    // cleanup
    drop(_block_sender);
}

#[tokio::test]
async fn run__new_block_subscription__sends_new_block() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, sender) = FakeApi::new();
    let db = FakeDB::new();
    let (source, source_sender) = FakeBlockSource::new();
    let mut srv = BlockAggregator::new(api, db, source);

    let expected_block = Block::random(&mut rng);
    let expected_height = BlockHeight::from(123u32);
    let mut watcher = StateWatcher::started();
    let (query, response) = BlockAggregatorQuery::new_block_subscription();

    // when
    sender.send(query).await.unwrap();
    let _ = srv.run(&mut watcher).await;
    let event = BlockSourceEvent::NewBlock(expected_height, expected_block.clone());
    source_sender.send(event).await.unwrap();
    let _ = srv.run(&mut watcher).await;

    // then
    let (actual_height, actual_block) = await_response_with_timeout(response)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(expected_block, actual_block);
    assert_eq!(expected_height, actual_height);

    // cleanup
    drop(source_sender);
}

#[tokio::test]
async fn run__new_block_subscription__does_not_send_syncing_blocks() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, sender) = FakeApi::new();
    let db = FakeDB::new();
    let (source, source_sender) = FakeBlockSource::new();
    let mut srv = BlockAggregator::new(api, db, source);

    let block = Block::random(&mut rng);
    let height = BlockHeight::from(123u32);
    let mut watcher = StateWatcher::started();
    let (query, response) = BlockAggregatorQuery::new_block_subscription();

    // when
    sender.send(query).await.unwrap();
    let _ = srv.run(&mut watcher).await;
    let event = BlockSourceEvent::OldBlock(height, block);
    source_sender.send(event).await.unwrap();
    let _ = srv.run(&mut watcher).await;

    // then
    let res = await_response_with_timeout(response).await;
    assert!(res.is_err(), "should have timed out");

    // cleanup
    drop(source_sender);
}

async fn await_response_with_timeout<T>(mut response: Receiver<T>) -> Result<T, Elapsed> {
    tokio::time::timeout(tokio::time::Duration::from_secs(1), async {
        loop {
            if let Ok(result) = response.try_recv() {
                return result;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    })
    .await
}
