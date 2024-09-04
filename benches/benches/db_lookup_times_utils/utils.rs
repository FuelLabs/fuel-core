use crate::db_lookup_times_utils::full_block_table::{
    BenchDatabase,
    BenchDbColumn,
};
use anyhow::anyhow;
use fuel_core::{
    database::database_description::DatabaseDescription,
    state::rocks_db::RocksDb,
};
use fuel_core_storage::kv_store::{
    KeyValueInspect,
    StorageColumn,
};
use fuel_core_types::{
    blockchain::block::{
        Block,
        CompressedBlock,
    },
    fuel_tx::Transaction,
    fuel_types::{
        BlockHeight,
        ChainId,
    },
};
use itertools::Itertools;
use rand::{
    rngs::ThreadRng,
    Rng,
};
use std::path::PathBuf;

pub type Result<T> = core::result::Result<T, anyhow::Error>;

pub fn get_random_block_height(rng: &mut ThreadRng, block_count: u32) -> BlockHeight {
    BlockHeight::from(rng.gen_range(0..block_count))
}

fn get_base_path() -> PathBuf {
    PathBuf::from("./db_benchmarks")
}

pub fn get_base_path_from_method(method: &str) -> PathBuf {
    get_base_path().join(PathBuf::from(format!("{method}").as_str()))
}

pub fn open_rocks_db<T: DatabaseDescription>(
    block_count: u32,
    tx_count: u32,
    method: &str,
) -> Result<RocksDb<T>> {
    let path = get_base_path_from_method(method).join(PathBuf::from(
        format!("./{block_count}/{tx_count}").as_str(),
    ));
    let db = RocksDb::default_open(&path, None)?;
    Ok(db)
}

pub fn chain_id() -> ChainId {
    ChainId::default()
}

pub fn get_full_block(
    database: &RocksDb<BenchDatabase>,
    height: &BlockHeight,
) -> Result<Option<Block>> {
    let height_key = height.to_bytes();
    let raw_block = database
        .get(&height_key, BenchDbColumn::FullFuelBlocks)?
        .ok_or(anyhow!("empty raw full block"))?;

    let block: Block = postcard::from_bytes(raw_block.as_slice())?;
    Ok(Some(block))
}

pub fn multi_get_block(
    database: &RocksDb<BenchDatabase>,
    height: BlockHeight,
) -> Result<Block> {
    let height_key = height.to_bytes();

    let raw_block = database
        .get(&height_key, BenchDbColumn::FuelBlocks)?
        .ok_or(anyhow!("empty raw block"))?;
    let block: CompressedBlock = postcard::from_bytes(raw_block.as_slice())?;
    let tx_ids = block.transactions().iter();
    let raw_txs = database.multi_get(BenchDbColumn::Transactions.id(), tx_ids)?;
    let txs: Vec<Transaction> = raw_txs
        .iter()
        .flatten()
        .into_iter()
        .map(|raw_tx| postcard::from_bytes::<Transaction>(raw_tx.as_slice()))
        .try_collect()?;

    Ok(block.uncompress(txs))
}

pub fn headers_and_tx_get_block(
    database: &RocksDb<BenchDatabase>,
    height: BlockHeight,
) -> Result<Block> {
    let height_key = height.to_bytes();

    let raw_block = database
        .get(&height_key, BenchDbColumn::FuelBlocks)?
        .ok_or(anyhow!("empty raw block"))?;
    let block: CompressedBlock = postcard::from_bytes(raw_block.as_slice())?;

    let txs: Vec<Transaction> = block
        .transactions()
        .iter()
        .map(|tx_id| {
            let raw_tx = database
                .get(tx_id.as_slice(), BenchDbColumn::Transactions)?
                .ok_or(anyhow!("empty transaction"))?;
            postcard::from_bytes::<Transaction>(raw_tx.as_slice())
                .map_err(|err| anyhow!(err))
        })
        .try_collect()?;

    Ok(block.uncompress(txs))
}
