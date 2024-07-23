use fuel_core_types::fuel_compression::{
    access::*,
    tables,
    Key,
    KeyPerTable,
    RawKey,
    Table,
};
use tokio::sync::mpsc;

use fuel_core_types::blockchain::block::Block;

use crate::{
    block_section::{
        ChangesPerTable,
        WriteTo,
    },
    db::RocksDb,
    CompressedBlockPayload,
    Header,
};

use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_compression::{
        Compactable,
        CompactionContext,
    },
    fuel_tx::Transaction,
    tai64::Tai64,
};

/// Task handle
pub struct Task {
    request_receiver: mpsc::Receiver<TaskRequest>,
}

pub enum TaskRequest {
    Compress {
        block: Block,
        response: mpsc::Sender<Result<Vec<u8>, CompressError>>,
    },
}

#[derive(Debug)]
pub enum CompressError {
    /// Only the next sequential block can be compressed
    NotLatest,
    Other(anyhow::Error),
}
impl From<anyhow::Error> for CompressError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}

async fn run(mut db: RocksDb, mut request_receiver: mpsc::Receiver<TaskRequest>) {
    while let Some(req) = request_receiver.recv().await {
        match req {
            TaskRequest::Compress { block, response } => {
                let reply = compress(&mut db, block);
                response.send(reply).await.expect("Failed to respond");
            }
        }
    }
}

pub fn compress(db: &mut RocksDb, block: Block) -> Result<Vec<u8>, CompressError> {
    if *block.header().height() != db.next_block_height()? {
        return Err(CompressError::NotLatest);
    }

    let target = block.transactions().to_vec();

    let start_keys = db.start_keys()?;
    let key_limits = target.count();
    let safe_keys_start = start_keys.offset_by(key_limits);
    let mut ctx = Ctx {
        db,
        start_keys,
        next_keys: start_keys,
        safe_keys_start,
        changes: ChangesPerTable::from_start_keys(start_keys),
    };
    let transactions = target.compact(&mut ctx)?;
    let registrations = ctx.changes;

    // Apply changes to the db
    // TODO: these should be done in a single atomic transaction
    registrations.write_to_db(db)?;
    db.increment_block_height()?;

    // Construct the actual compacted block
    let compact = CompressedBlockPayload {
        registrations,
        header: Header {
            da_height: block.header().da_height,
            prev_root: *block.header().prev_root(),
            consensus_parameters_version: block.header().consensus_parameters_version,
            state_transition_bytecode_version: block
                .header()
                .state_transition_bytecode_version,
            height: *block.header().height(),
            time: block.header().time(),
        },
        transactions,
    };

    let version = 0u8;

    let compressed =
        postcard::to_allocvec(&(version, compact)).expect("Serialization cannot fail");

    Ok(compressed)
}

pub struct Ctx<'a> {
    db: &'a mut RocksDb,
    /// These are the keys where writing started
    start_keys: KeyPerTable,
    /// The next keys to use for each table
    next_keys: KeyPerTable,
    /// Keys in range next_keys..safe_keys_start
    /// could be overwritten by the compaction,
    /// and cannot be used for new values.
    safe_keys_start: KeyPerTable,
    changes: ChangesPerTable,
}

impl<'a> Ctx<'a> {
    /// Convert a value to a key
    /// If necessary, store the value in the changeset and allocate a new key.
    fn value_to_key<T: Table>(&mut self, value: T::Type) -> anyhow::Result<Key<T>>
    where
        ChangesPerTable: AccessRef<T, WriteTo<T>> + AccessMut<T, WriteTo<T>>,
        KeyPerTable: AccessCopy<T, Key<T>> + AccessMut<T, Key<T>>,
    {
        // Check if the value is within the current changeset
        if let Some(key) =
            <ChangesPerTable as AccessRef<T, _>>::get(&self.changes).lookup_value(&value)
        {
            return Ok(key);
        }

        // Check if the registry contains this value already.
        if let Some(key) = self.db.index_lookup(&value)? {
            // Check if the value is in the possibly-overwritable range
            let start: Key<T> = self.start_keys.value();
            let end: Key<T> = self.safe_keys_start.value();
            if !key.is_between(start, end) {
                return Ok(key);
            }
        }

        // Allocate a new key for this
        let key = <KeyPerTable as AccessMut<T, Key<T>>>::get_mut(&mut self.next_keys)
            .take_next();

        <ChangesPerTable as AccessMut<T, _>>::get_mut(&mut self.changes)
            .values
            .push(value);
        Ok(key)
    }
}

impl<'a> CompactionContext for Ctx<'a> {
    fn to_key_AssetId(
        &mut self,
        value: [u8; 32],
    ) -> anyhow::Result<Key<tables::AssetId>> {
        self.value_to_key(value)
    }

    fn to_key_Address(
        &mut self,
        value: [u8; 32],
    ) -> anyhow::Result<Key<tables::Address>> {
        self.value_to_key(value)
    }

    fn to_key_ContractId(
        &mut self,
        value: [u8; 32],
    ) -> anyhow::Result<Key<tables::ContractId>> {
        self.value_to_key(value)
    }

    fn to_key_ScriptCode(
        &mut self,
        value: Vec<u8>,
    ) -> anyhow::Result<Key<tables::ScriptCode>> {
        self.value_to_key(value)
    }

    fn to_key_Witness(&mut self, value: Vec<u8>) -> anyhow::Result<Key<tables::Witness>> {
        self.value_to_key(value)
    }
}
