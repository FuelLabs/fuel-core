use crate::db_lookup_times_utils::{
    full_block_table::{
        BenchDatabase,
        BenchDbColumn,
    },
    utils::Result as DbLookupBenchResult,
};
use anyhow::anyhow;
use fuel_core::state::rocks_db::RocksDb;
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
    fuel_types::{
        BlockHeight,
        ChainId,
    },
};
use itertools::Itertools;

pub fn seed_compressed_blocks_and_transactions_matrix(
    database: &mut RocksDb<BenchDatabase>,
    block_count: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    seed_compressed_blocks_and_transactions(database, block_count, tx_count)
}

pub fn seed_full_block_matrix(
    database: &mut RocksDb<BenchDatabase>,
    block_count: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    seed_full_blocks(database, block_count, tx_count)
}

fn generate_bench_block(
    height: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<Block> {
    let header = PartialBlockHeader {
        application: Default::default(),
        consensus: ConsensusHeader::<Empty> {
            height,
            ..Default::default()
        },
    };
    let txs = generate_bench_transactions(tx_count);
    let block = PartialFuelBlock::new(header, txs);
    block
        .generate(&[], Default::default())
        .map_err(|err| anyhow!(err))
}

fn generate_bench_transactions(tx_count: u32) -> Vec<Transaction> {
    let mut txs = Vec::with_capacity(tx_count as usize);
    for _ in 0..tx_count {
        txs.push(Transaction::default_test_tx());
    }
    txs
}

fn height_key(block_height: BlockHeight) -> Vec<u8> {
    block_height.to_bytes().to_vec()
}

pub fn insert_compressed_block(
    database: &mut RocksDb<BenchDatabase>,
    height: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<Block> {
    let block = generate_bench_block(height, tx_count)?;

    let compressed_block = block.compress(&ChainId::default());
    let height_key = height_key(height);

    let raw_compressed_block = postcard::to_allocvec(&compressed_block)?.to_vec();
    let raw_transactions: Vec<(Bytes32, Vec<u8>)> = block
        .transactions()
        .iter()
        .map(|tx| -> DbLookupBenchResult<(Bytes32, Vec<u8>)> {
            let tx_id = tx.id(&ChainId::default());
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

pub fn insert_full_block(
    database: &mut RocksDb<BenchDatabase>,
    height: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<Block> {
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
        .map_err(|err| anyhow!(err))?;

    Ok(block)
}

fn seed_compressed_blocks_and_transactions(
    database: &mut RocksDb<BenchDatabase>,
    block_count: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    for block_number in 0..block_count.into() {
        insert_compressed_block(database, block_number.into(), tx_count)?;
    }

    Ok(())
}

fn seed_full_blocks(
    full_block_db: &mut RocksDb<BenchDatabase>,
    block_count: BlockHeight,
    tx_count: u32,
) -> DbLookupBenchResult<()> {
    for block_number in 0..block_count.into() {
        insert_full_block(full_block_db, block_number.into(), tx_count)?;
    }

    Ok(())
}
