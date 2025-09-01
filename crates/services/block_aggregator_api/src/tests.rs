use super::*;
use crate::blocks::Block;
use std::{
    collections::HashMap,
    future,
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
    map: HashMap<u64, Block>,
}

impl FakeDB {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn add_block(&mut self, id: u64, block: Block) {
        self.map.insert(id, block);
    }
}

impl BlockAggregatorDB for FakeDB {
    fn store_block(&mut self, block: Block) -> Result<()> {
        todo!()
    }

    fn get_block_range(&self, first: u64, last: u64) -> Result<Vec<Block>> {
        let mut blocks = vec![];
        for id in first..=last {
            if let Some(block) = self.map.get(&id) {
                blocks.push(*block);
            }
        }
        Ok(blocks)
    }
}

struct FakeBlockSource;

impl BlockSource for FakeBlockSource {
    async fn next_block(&mut self) -> Result<Block> {
        future::pending().await
    }
}

#[tokio::test]
async fn run__get_block_range__returns_expected_blocks() {
    // given
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let api = FakeApi { receiver };
    let mut db = FakeDB::new();
    db.add_block(1, Block);
    db.add_block(2, Block);
    db.add_block(3, Block);

    let source = FakeBlockSource;

    let mut srv = BlockAggregator::new(api, db, source);

    // when
    let mut watcher = StateWatcher::started();
    tokio::spawn(async move {
        let _ = srv.run(&mut watcher).await;
    });
    let (query, mut response) = BlockAggregatorQuery::get_block_range(2, 3);
    sender.send(query).await.unwrap();

    // then
    let mut buffer = vec![];
    let response = response.recv_many(&mut buffer, 3).await;

    // TODO: Check values
    assert_eq!(buffer.len(), 2);
}
