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
use fuel_core_storage::{
    column::Column,
    kv_store::{
        KeyValueInspect,
        StorageColumn,
    },
    tables::FullFuelBlocks,
    StorageAsRef,
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
use std::{
    borrow::Cow,
    path::Path,
};

pub fn thread_rng() -> ThreadRng {
    rand::thread_rng()
}

pub fn get_random_block_height(rng: &mut ThreadRng, block_count: u32) -> BlockHeight {
    BlockHeight::from(rng.gen_range(0..block_count))
}

pub fn open_db(block_count: u32, tx_count: u32, method: &str) -> Database {
    Database::open_rocksdb(
        Path::new(format!("./{block_count}/{method}/{tx_count}").as_str()),
        None, // no caching
        StateRewindPolicy::NoRewind,
    )
    .unwrap()
}

pub fn open_raw_rocksdb(
    block_count: u32,
    tx_count: u32,
    method: &str,
) -> RocksDb<OnChain> {
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
    view: &IterableKeyValueView<Column>,
    height: &BlockHeight,
) -> Result<Option<Block>, anyhow::Error> {
    let db_block = view.storage::<FullFuelBlocks>().get(height)?;
    if let Some(block) = db_block {
        Ok(Some(Cow::into_owned(block)))
    } else {
        Ok(None)
    }
}

pub fn multi_get_block(database: &RocksDb<OnChain>, height: BlockHeight) {
    let height_key = height.to_bytes();

    let raw_block = database
        .get(&height_key, Column::FuelBlocks)
        .unwrap()
        .unwrap();
    let block: CompressedBlock = postcard::from_bytes(raw_block.as_slice()).unwrap();
    let tx_ids = block.transactions().iter();
    let raw_txs = database
        .multi_get(Column::Transactions.id(), tx_ids)
        .unwrap();
    for raw_tx in raw_txs.iter().flatten() {
        let _: Transaction = postcard::from_bytes(raw_tx.as_slice()).unwrap();
    }
}
