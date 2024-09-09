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
    fuel_types::BlockHeight,
};
use itertools::Itertools;
use rand::{
    rngs::ThreadRng,
    Rng,
};
use std::path::Path;
use strum_macros::AsRefStr;

pub use anyhow::Result;

pub fn get_random_block_height(
    rng: &mut ThreadRng,
    block_count: BlockHeight,
) -> BlockHeight {
    BlockHeight::from(rng.gen_range(0..block_count.into()))
}

pub fn open_rocks_db<Description: DatabaseDescription>(
    path: &Path,
) -> Result<RocksDb<Description>> {
    let db = RocksDb::default_open(path, None)?;
    Ok(db)
}

#[derive(Copy, Clone, AsRefStr)]
pub enum LookupMethod {
    FullBlock,
    MultiGet,
    HeaderAndTx,
}

impl LookupMethod {
    pub fn get_block(
        &self,
        database: &RocksDb<BenchDatabase>,
        height: BlockHeight,
    ) -> Result<Block> {
        match self {
            LookupMethod::FullBlock => get_block_full_block_method(database, height),
            LookupMethod::MultiGet => get_block_multi_get_method(database, height),
            LookupMethod::HeaderAndTx => {
                get_block_headers_and_tx_method(database, height)
            }
        }
    }
}

fn get_block_full_block_method(
    database: &RocksDb<BenchDatabase>,
    height: BlockHeight,
) -> Result<Block> {
    let height_key = height.to_bytes();
    let raw_block = database
        .get(&height_key, BenchDbColumn::FullFuelBlocks)?
        .ok_or(anyhow!("empty raw full block"))?;

    let block: Block = postcard::from_bytes(raw_block.as_slice())?;
    Ok(block)
}

fn get_block_multi_get_method(
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
        .map(|raw_tx| postcard::from_bytes::<Transaction>(raw_tx.as_slice()))
        .try_collect()?;

    Ok(block.uncompress(txs))
}

fn get_block_headers_and_tx_method(
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
