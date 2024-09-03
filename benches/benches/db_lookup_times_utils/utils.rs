use crate::db_lookup_times_utils::full_block_table::{
    FullFuelBlockDesc,
    FullFuelBlocksColumns,
};
use anyhow::anyhow;
use fuel_core::{
    database::{
        database_description::{
            on_chain::OnChain,
            DatabaseDescription,
        },
        metadata::MetadataTable,
        Database,
    },
    state::{
        historical_rocksdb::StateRewindPolicy,
        rocks_db::RocksDb,
    },
};
use fuel_core_storage::{
    column::Column,
    kv_store::{
        KeyValueInspect,
        StorageColumn,
    },
    Error as StorageError,
    StorageInspect,
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
use rand::{
    rngs::ThreadRng,
    Rng,
};
use std::path::Path;

pub fn get_random_block_height(rng: &mut ThreadRng, block_count: u32) -> BlockHeight {
    BlockHeight::from(rng.gen_range(0..block_count))
}

pub fn open_db<T: DatabaseDescription>(
    block_count: u32,
    tx_count: u32,
    method: &str,
) -> Database<T>
where
    Database<T>: StorageInspect<MetadataTable<T>, Error = StorageError>,
{
    Database::open_rocksdb(
        Path::new(format!("./{block_count}/{method}/{tx_count}").as_str()),
        None, // no caching
        StateRewindPolicy::NoRewind,
    )
    .unwrap()
}

pub fn open_raw_rocksdb<T: DatabaseDescription>(
    block_count: u32,
    tx_count: u32,
    method: &str,
) -> RocksDb<T> {
    RocksDb::default_open(
        Path::new(format!("./{block_count}/{method}/{tx_count}").as_str()),
        None,
    )
    .unwrap()
}

pub fn chain_id() -> ChainId {
    ChainId::default()
}

pub fn get_full_block(
    database: &RocksDb<FullFuelBlockDesc>,
    height: &BlockHeight,
) -> anyhow::Result<Option<Block>> {
    let height_key = height.to_bytes();
    let raw_block = database
        .get(&height_key, FullFuelBlocksColumns::FullFuelBlocks)?
        .ok_or(anyhow!("empty raw full block"))?;

    let block: Block = postcard::from_bytes(raw_block.as_slice())?;
    Ok(Some(block))
}

pub fn multi_get_block(
    database: &RocksDb<OnChain>,
    height: BlockHeight,
) -> anyhow::Result<()> {
    let height_key = height.to_bytes();

    let raw_block = database
        .get(&height_key, Column::FuelBlocks)?
        .ok_or(anyhow!("empty raw block"))?;
    let block: CompressedBlock = postcard::from_bytes(raw_block.as_slice())?;
    let tx_ids = block.transactions().iter();
    let raw_txs = database.multi_get(Column::Transactions.id(), tx_ids)?;
    for raw_tx in raw_txs.iter().flatten() {
        let _: Transaction = postcard::from_bytes(raw_tx.as_slice())?;
    }

    Ok(())
}
