use anyhow::anyhow;
use fuel_core::{
    database::{
        database_description::on_chain::OnChain,
        Database,
    },
    state::{
        historical_rocksdb::StateRewindPolicy,
        rocks_db::RocksDb,
        IterableKeyValueView,
    },
};
use fuel_core_storage::{column::Column, kv_store::{
    KeyValueInspect,
    StorageColumn,
}, Error, StorageAsMut, StorageAsRef, StorageInspect};
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
use std::{
    borrow::Cow,
    path::Path,
};
use fuel_core::database::database_description::DatabaseDescription;
use fuel_core::database::metadata::MetadataTable;
use crate::db_lookup_times_utils::full_block_table::{FullFuelBlocks, FullFuelBlocksColumns};

pub fn get_random_block_height(rng: &mut ThreadRng, block_count: u32) -> BlockHeight {
    BlockHeight::from(rng.gen_range(0..block_count))
}

pub fn open_db<T: DatabaseDescription>(block_count: u32, tx_count: u32, method: &str) -> Database<T>
where
    Database<T>: StorageInspect<MetadataTable<T>, Error = Error>,
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
    view: &IterableKeyValueView<FullFuelBlocksColumns>,
    height: &BlockHeight,
) -> anyhow::Result<Option<Block>> {
    let db_block = view.storage::<FullFuelBlocks>().get(height)?;
    if let Some(block) = db_block {
        Ok(Some(Cow::into_owned(block)))
    } else {
        Ok(None)
    }
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
