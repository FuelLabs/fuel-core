use crate::db_lookup_times_utils::{
    matrix::matrix,
    utils::{
        chain_id,
        open_raw_rocksdb,
    },
};
use fuel_core::{
    database::database_description::on_chain::OnChain,
    state::rocks_db::RocksDb,
};
use fuel_core_storage::{
    column::Column,
    kv_store::{
        KeyValueMutate,
        Value,
    },
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
            PartialFuelBlock,
        },
        header::{
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::Empty,
    },
    fuel_tx::{
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
};

pub fn seed_compressed_blocks_and_transactions_matrix(method: &str) {
    for (block_count, tx_count) in matrix() {
        let mut database = open_raw_rocksdb(block_count, tx_count, method);
        let _ =
            seed_compressed_blocks_and_transactions(&mut database, block_count, tx_count);
    }
}

pub fn seed_full_block_matrix() {
    for (block_count, tx_count) in matrix() {
        let mut database = open_raw_rocksdb(block_count, tx_count, "full_block");
        seed_full_blocks(&mut database, block_count, tx_count);
    }
}

fn generate_bench_block(height: u32, tx_count: u32) -> Block {
    let header = PartialBlockHeader {
        application: Default::default(),
        consensus: ConsensusHeader::<Empty> {
            height: BlockHeight::from(height),
            ..Default::default()
        },
    };
    let txes = generate_bench_transactions(tx_count);
    let block = PartialFuelBlock::new(header, txes);
    block.generate(&[], Default::default()).unwrap()
}

fn generate_bench_transactions(tx_count: u32) -> Vec<Transaction> {
    let mut txes = vec![];
    for _ in 0..tx_count {
        txes.push(Transaction::default_test_tx());
    }
    txes
}

fn height_key(block_height: u32) -> Vec<u8> {
    BlockHeight::from(block_height).to_bytes().to_vec()
}

fn insert_compressed_block(
    database: &mut RocksDb<OnChain>,
    height: u32,
    tx_count: u32,
) -> Block {
    let block = generate_bench_block(height, tx_count);

    let compressed_block = block.compress(&chain_id());
    let height_key = height_key(height);

    let raw_compressed_block = postcard::to_allocvec(&compressed_block).unwrap().to_vec();
    let raw_transactions = block
        .transactions()
        .iter()
        .map(|tx| {
            (
                tx.id(&chain_id()),
                postcard::to_allocvec(tx).unwrap().to_vec(),
            )
        })
        .collect::<Vec<_>>();

    // 1. insert into CompressedBlocks table
    database
        .put(
            height_key.as_slice(),
            Column::FuelBlocks,
            Value::new(raw_compressed_block),
        )
        .unwrap();
    // 2. insert into Transactions table
    for (tx_id, tx) in raw_transactions {
        database
            .put(tx_id.as_slice(), Column::Transactions, Value::new(tx))
            .unwrap();
    }

    block
}

fn insert_full_block(database: &mut RocksDb<OnChain>, height: u32, tx_count: u32) {
    // we seed compressed blocks and transactions to not affect individual
    // lookup times
    let block = insert_compressed_block(database, height, tx_count);

    let height_key = height_key(height);
    let raw_full_block = postcard::to_allocvec(&block).unwrap().to_vec();

    database
        .put(
            height_key.as_slice(),
            Column::FullFuelBlocks,
            Value::new(raw_full_block),
        )
        .unwrap();
}

fn seed_compressed_blocks_and_transactions(
    database: &mut RocksDb<OnChain>,
    block_count: u32,
    tx_count: u32,
) -> Vec<Block> {
    let mut blocks = vec![];
    for block_number in 0..block_count {
        blocks.push(insert_compressed_block(database, block_number, tx_count));
    }
    blocks
}

fn seed_full_blocks(database: &mut RocksDb<OnChain>, block_count: u32, tx_count: u32) {
    for block_number in 0..block_count {
        insert_full_block(database, block_number, tx_count);
    }
}
