#![allow(non_snake_case)]

use super::*;
use crate::blocks::Block;
use fuel_core_services::stream::BoxStream;
use futures_util::StreamExt;
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
use tokio::sync::mpsc::{
    Receiver,
    Sender,
};

struct FakeApi {
    receiver: Receiver<BlockAggregatorQuery>,
}

impl FakeApi {
    fn new() -> (Self, Sender<BlockAggregatorQuery>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let api = Self { receiver };
        (api, sender)
    }
}

impl BlockAggregatorApi for FakeApi {
    async fn await_query(&mut self) -> Result<BlockAggregatorQuery> {
        Ok(self.receiver.recv().await.unwrap())
    }
}

struct FakeDB {
    map: Arc<Mutex<HashMap<u64, Block>>>,
}

impl FakeDB {
    fn new() -> Self {
        let map = Arc::new(Mutex::new(HashMap::new()));
        Self { map }
    }

    fn add_block(&mut self, id: u64, block: Block) {
        self.map.lock().unwrap().insert(id, block);
    }

    fn clone_inner(&self) -> Arc<Mutex<HashMap<u64, Block>>> {
        self.map.clone()
    }
}

impl BlockAggregatorDB for FakeDB {
    async fn store_block(&mut self, id: u64, block: Block) -> Result<()> {
        self.map.lock().unwrap().insert(id, block);
        Ok(())
    }

    async fn get_block_range(&self, first: u64, last: u64) -> Result<BoxStream<Block>> {
        let mut blocks = vec![];
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
        Ok(Box::pin(futures_util::stream::iter(blocks)))
    }
}

struct FakeBlockSource {
    blocks: Receiver<(u64, Block)>,
}

impl FakeBlockSource {
    fn new() -> (Self, Sender<(u64, Block)>) {
        let (_sender, receiver) = tokio::sync::mpsc::channel(1);
        let _self = Self { blocks: receiver };
        (_self, _sender)
    }
}

impl BlockSource for FakeBlockSource {
    async fn next_block(&mut self) -> Result<(u64, Block)> {
        Ok(self.blocks.recv().await.unwrap())
    }
}

#[tokio::test]
async fn run__get_block_range__returns_expected_blocks() {
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, sender) = FakeApi::new();
    let mut db = FakeDB::new();
    db.add_block(1, Block::random(&mut rng));
    db.add_block(2, Block::random(&mut rng));
    db.add_block(3, Block::random(&mut rng));

    let (source, _) = FakeBlockSource::new();

    let mut srv = BlockAggregator::new(api, db, source);

    // when
    let mut watcher = StateWatcher::started();
    tokio::spawn(async move {
        let _ = srv.run(&mut watcher).await;
    });
    let (query, response) = BlockAggregatorQuery::get_block_range(2, 3);
    sender.send(query).await.unwrap();

    // then
    let stream = response.await.unwrap();
    let blocks = stream.collect::<Vec<Block>>().await;

    // TODO: Check values
    assert_eq!(blocks.len(), 2);
}

#[tokio::test]
async fn run__new_block_gets_added_to_db() {
    // let _ = tracing_subscriber::fmt()
    //     .with_max_level(tracing::Level::DEBUG)
    //     .try_init();
    let mut rng = StdRng::seed_from_u64(42);
    // given
    let (api, _sender) = FakeApi::new();
    let db = FakeDB::new();
    let db_map = db.clone_inner();
    let (source, source_sender) = FakeBlockSource::new();
    let mut srv = BlockAggregator::new(api, db, source);

    // when
    let mut watcher = StateWatcher::started();
    tokio::spawn(async move {
        let _ = srv.run(&mut watcher).await;
    });
    let block = Block::random(&mut rng);
    let id = 123u64;
    source_sender.send((id, block.clone())).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // then
    let actual = db_map.lock().unwrap().get(&id).unwrap().clone();
    assert_eq!(block, actual);
}
