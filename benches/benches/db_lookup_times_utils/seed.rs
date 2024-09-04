use crate::db_lookup_times_utils::{
    full_block_table::{
        BenchDatabase,
        BenchDbColumn,
    },
    matrix::matrix,
    utils::{
        chain_id,
        open_rocks_db,
        Result as DbLookupBenchResult,
    },
};
use anyhow::anyhow;
use fuel_core::{
    database::database_description::DatabaseDescription,
    state::rocks_db::RocksDb,
};
use fuel_core_storage::kv_store::{
    KeyValueMutate,
    Value,
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
        Bytes32,
        Transaction,
        UniqueIdentifier,
    },
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use std::fs;

fn seed_matrix<SeedClosure, Description>(
    method: &str,
    seed_closure: SeedClosure,
) -> DbLookupBenchResult<impl FnOnce()>
where
    SeedClosure: Fn(&mut RocksDb<Description>, u32, u32) -> DbLookupBenchResult<()>,
    Description: DatabaseDescription,
{
    let mut databases = vec![];

    for (block_count, tx_count) in matrix() {
        let (mut database, path) = open_rocks_db(block_count, tx_count, method)?;
        seed_closure(&mut database, block_count, tx_count)?;
        databases.push(path);
    }

    Ok(|| {
        for path in databases {
            fs::remove_dir_all(path).unwrap();
        }
    })
}

pub fn seed_compressed_blocks_and_transactions_matrix(
    method: &str,
) -> DbLookupBenchResult<impl FnOnce()> {
    seed_matrix(method, |database, block_count, tx_count| {
        seed_compressed_blocks_and_transactions(database, block_count, tx_count)
    })
}

pub fn seed_full_block_matrix() -> DbLookupBenchResult<impl FnOnce()> {
    seed_matrix("full_block", |database, block_count, tx_count| {
        seed_full_blocks(database, block_count, tx_count)
    })
}

fn generate_bench_block(height: u32, tx_count: u32) -> DbLookupBenchResult<Block> {
    let header = PartialBlockHeader {
        application: Default::default(),
        consensus: ConsensusHeader::<Empty> {
            height: BlockHeight::from(height),
            ..Default::default()
        },
    };
    let txes = generate_bench_transactions(tx_count);
    let block = PartialFuelBlock::new(header, txes);
    block
        .generate(&[], Default::default())
        .map_err(|err| anyhow!(err))
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
    database: &mut RocksDb<BenchDatabase>,
    height: u32,
    tx_count: u32,
) -> DbLookupBenchResult<Block> {
    let block = generate_bench_block(height, tx_count)?;

    let compressed_block = block.compress(&chain_id());
    let height_key = height_key(height);

    let raw_compressed_block = postcard::to_allocvec(&compressed_block)?.to_vec();
    let raw_transactions: Vec<(Bytes32, Vec<u8>)> = block
        .transactions()
        .iter()
        .map(|tx| -> DbLookupBenchResult<(Bytes32, Vec<u8>)> {
            let tx_id = tx.id(&chain_id());
            let raw_tx = postcard::to_allocvec(tx)?.to_vec();
            Ok((tx_id, raw_tx))
        })
        .try_collect()?;

    // 1. insert into CompressedBlocks table
    database.put(
        height_key.as_slice(),
        BenchDbColumn::FuelBlocks,
        Value::new(raw_compressed_block),
    )?;
    // 2. insert into Transactions table
    for (tx_id, tx) in raw_transactions {
        database.put(
            tx_id.as_slice(),
            BenchDbColumn::Transactions,
            Value::new(tx),
        )?;
    }

    Ok(block)
}

fn insert_full_block(
    database: &mut RocksDb<BenchDatabase>,
    height: u32,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    // we seed compressed blocks and transactions to not affect individual
    // lookup times
    let block = insert_compressed_block(database, height, tx_count)?;

    let height_key = height_key(height);
    let raw_full_block = postcard::to_allocvec(&block)?.to_vec();

    database
        .put(
            height_key.as_slice(),
            BenchDbColumn::FullFuelBlocks,
            Value::new(raw_full_block),
        )
        .map_err(|err| anyhow!(err))
}

fn seed_compressed_blocks_and_transactions(
    database: &mut RocksDb<BenchDatabase>,
    block_count: u32,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    for block_number in 0..block_count {
        insert_compressed_block(database, block_number, tx_count)?;
    }

    Ok(())
}

fn seed_full_blocks(
    full_block_db: &mut RocksDb<BenchDatabase>,
    block_count: u32,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    for block_number in 0..block_count {
        insert_full_block(full_block_db, block_number, tx_count)?;
    }

    Ok(())
}
