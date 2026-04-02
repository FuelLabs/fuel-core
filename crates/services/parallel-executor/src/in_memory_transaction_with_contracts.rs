use fuel_core_storage::{
    Error as StorageError,
    Result as StorageResult,
    blueprint::BlueprintCodec,
    codec::Decode,
    column::Column,
    kv_store::{
        BatchOperations,
        KeyValueInspect,
        KeyValueMutate,
        StorageColumn,
        Value,
        WriteOperation,
    },
    structured_storage::TableWithBlueprint,
    tables::{
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
    },
    transactional::{
        Changes,
        Modifiable,
    },
};
use fuel_core_types::{
    fuel_merkle::storage::Mappable,
    fuel_types::ContractId,
};
use fxhash::FxHashMap;
use std::collections::btree_map;

fn is_contract(key: &[u8], column: Column) -> Option<ContractId> {
    match column {
        Column::ContractsRawCode => {
            let key =
                <<ContractsRawCode as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsRawCode,
                >>::KeyCodec::decode(key)
                .expect("Failed to decode ContractsRawCode key");
            Some(key)
        }
        Column::ContractsState => {
            let key: <ContractsState as Mappable>::OwnedKey =
                <<ContractsState as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsState,
                >>::KeyCodec::decode(key)
                .expect("Failed to decode ContractsState key");
            Some(*key.contract_id())
        }
        Column::ContractsLatestUtxo => {
            let key =
                <<ContractsLatestUtxo as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsLatestUtxo,
                >>::KeyCodec::decode(key).expect("Failed to decode ContractsLatestUtxo key");
            Some(key)
        }
        Column::ContractsAssets => {
            let key: <ContractsAssets as Mappable>::OwnedKey =
                <<ContractsAssets as TableWithBlueprint>::Blueprint as BlueprintCodec<
                    ContractsAssets,
                >>::KeyCodec::decode(key)
                .expect("Failed to decode ContractsAssets key");
            Some(*key.contract_id())
        }
        _ => None,
    }
}

/// In memory transaction accumulates `Changes` over the storage.
/// It tracks changes for the contracts separately.
#[derive(Default, Debug, Clone)]
pub struct InMemoryTransactionWithContracts<S> {
    pub(crate) changes: Changes,
    pub(crate) changes_per_contract: FxHashMap<ContractId, Changes>,
    pub(crate) storage: S,
}

impl<S> InMemoryTransactionWithContracts<S> {
    /// Creates a new in-memory transaction over the provided storage.
    pub fn new(storage: S, changes_per_contract: FxHashMap<ContractId, Changes>) -> Self {
        Self {
            changes: Changes::default(),
            changes_per_contract,
            storage,
        }
    }

    /// Consumes the transaction and returns the accumulated changes.
    pub fn into_changes(self) -> (Changes, FxHashMap<ContractId, Changes>) {
        (self.changes, self.changes_per_contract)
    }
}

impl<Storage> Modifiable for InMemoryTransactionWithContracts<Storage> {
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        for (column_u32, value) in changes.into_iter() {
            let column = Column::try_from(column_u32).map_err(|e| {
                StorageError::Other(anyhow::anyhow!(
                    "Failed to convert column id `{}` to Column: {}",
                    column_u32,
                    e
                ))
            })?;

            for (k, v) in value {
                let changes = if let Some(contract_id) = is_contract(k.as_ref(), column) {
                    self.changes_per_contract.entry(contract_id).or_default()
                } else {
                    &mut self.changes
                };

                changes.entry(column_u32).or_default().insert(k, v);
            }
        }
        Ok(())
    }
}

impl<S> InMemoryTransactionWithContracts<S>
where
    S: KeyValueInspect<Column = Column>,
{
    fn get_from_changes(&self, key: &[u8], column: Column) -> Option<&WriteOperation> {
        let changes = if let Some(contract_id) = is_contract(key, column) {
            self.changes_per_contract.get(&contract_id)?
        } else {
            &self.changes
        };
        changes.get(&column.id()).and_then(|btree| btree.get(key))
    }
}

impl<S> KeyValueInspect for InMemoryTransactionWithContracts<S>
where
    S: KeyValueInspect<Column = Column>,
{
    type Column = Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(_) => Ok(true),
                WriteOperation::Remove => Ok(false),
            }
        } else {
            self.storage.exists(key, column)
        }
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(value) => Ok(Some(value.len())),
                WriteOperation::Remove => Ok(None),
            }
        } else {
            self.storage.size_of_value(key, column)
        }
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(value) => Ok(Some(value.clone())),
                WriteOperation::Remove => Ok(None),
            }
        } else {
            self.storage.get(key, column)
        }
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        offset: usize,
        buf: &mut [u8],
    ) -> StorageResult<bool> {
        if let Some(operation) = self.get_from_changes(key, column) {
            match operation {
                WriteOperation::Insert(value) => {
                    let bytes_len = value.as_ref().len();
                    let start = offset;
                    let buf_len = buf.len();
                    let end = offset.saturating_add(buf.len());

                    if end > bytes_len {
                        return Err(anyhow::anyhow!(
                            "Offset `{offset}` + buf_len `{buf_len}` read until {end} which is out of bounds `{bytes_len}` for key `{:?}`",
                            key
                        )
                            .into());
                    }

                    let starting_from_offset = &value.as_ref()[start..end];
                    buf[..].copy_from_slice(starting_from_offset);
                    Ok(true)
                }
                WriteOperation::Remove => Ok(false),
            }
        } else {
            self.storage.read(key, column, offset, buf)
        }
    }
}

impl<S> KeyValueMutate for InMemoryTransactionWithContracts<S>
where
    S: KeyValueInspect<Column = Column>,
{
    fn put(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<()> {
        let k = key.to_vec().into();
        let changes = if let Some(contract_id) = is_contract(key, column) {
            self.changes_per_contract.entry(contract_id).or_default()
        } else {
            &mut self.changes
        };
        changes
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Insert(value));
        Ok(())
    }

    fn replace(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        let k = key.to_vec().into();
        let changes = if let Some(contract_id) = is_contract(key, column) {
            self.changes_per_contract.entry(contract_id).or_default()
        } else {
            &mut self.changes
        };
        let entry = changes.entry(column.id()).or_default().entry(k);

        match entry {
            btree_map::Entry::Occupied(mut occupied) => {
                let old = occupied.insert(WriteOperation::Insert(value));

                match old {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
            btree_map::Entry::Vacant(vacant) => {
                vacant.insert(WriteOperation::Insert(value));
                self.storage.get(key, column)
            }
        }
    }

    fn write(
        &mut self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize> {
        let k = key.to_vec().into();
        let changes = if let Some(contract_id) = is_contract(key, column) {
            self.changes_per_contract.entry(contract_id).or_default()
        } else {
            &mut self.changes
        };
        changes
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Insert(Value::from(buf)));
        Ok(buf.len())
    }

    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        let k = key.to_vec().into();
        let changes = if let Some(contract_id) = is_contract(key, column) {
            self.changes_per_contract.entry(contract_id).or_default()
        } else {
            &mut self.changes
        };
        let entry = changes.entry(column.id()).or_default().entry(k);

        match entry {
            btree_map::Entry::Occupied(mut occupied) => {
                let old = occupied.insert(WriteOperation::Remove);

                match old {
                    WriteOperation::Insert(value) => Ok(Some(value)),
                    WriteOperation::Remove => Ok(None),
                }
            }
            btree_map::Entry::Vacant(vacant) => {
                vacant.insert(WriteOperation::Remove);
                self.storage.get(key, column)
            }
        }
    }

    fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        let k = key.to_vec().into();
        let changes = if let Some(contract_id) = is_contract(key, column) {
            self.changes_per_contract.entry(contract_id).or_default()
        } else {
            &mut self.changes
        };
        changes
            .entry(column.id())
            .or_default()
            .insert(k, WriteOperation::Remove);
        Ok(())
    }
}

impl<S> BatchOperations for InMemoryTransactionWithContracts<S>
where
    S: KeyValueInspect<Column = Column>,
{
    fn batch_write<I>(&mut self, column: Column, entries: I) -> StorageResult<()>
    where
        I: Iterator<Item = (Vec<u8>, WriteOperation)>,
    {
        entries.for_each(|(key, operation)| {
            let changes = if let Some(contract_id) = is_contract(&key, column) {
                self.changes_per_contract.entry(contract_id).or_default()
            } else {
                &mut self.changes
            };
            let btree = changes.entry(column.id()).or_default();
            btree.insert(key.into(), operation);
        });
        Ok(())
    }
}
