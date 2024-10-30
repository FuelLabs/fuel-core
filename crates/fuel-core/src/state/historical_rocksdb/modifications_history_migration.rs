use std::marker::PhantomData;

use fuel_core_storage::{
    blueprint::BlueprintInspect,
    kv_store::StorageColumn,
    structured_storage::{
        test::InMemoryStorage,
        StructuredStorage,
        TableWithBlueprint,
    },
    transactional::{
        Changes,
        ConflictPolicy,
        InMemoryTransaction,
        ReferenceBytesKey,
        StorageTransaction,
    },
};

use crate::{
    database::database_description::DatabaseDescription,
    state::{
        historical_rocksdb::{
            description::Column,
            modifications_history::{
                ModificationsHistoryV1,
                ModificationsHistoryV2,
            },
        },
        in_memory::memory_store::MemoryStore,
        StorageResult,
    },
};
use fuel_core_storage::codec::Decode;

pub struct MigrationState<Description> {
    changes: Changes,
    // The height up to which the migration can be performed, included.
    last_height_to_be_migrated: Option<u64>,
    _description: PhantomData<Description>,
}

impl<Description> MigrationState<Description> {
    pub fn new() -> Self {
        Self {
            changes: Changes::default(),
            last_height_to_be_migrated: Some(u64::MAX),
            _description: PhantomData,
        }
    }

    pub fn is_migration_in_progress(&self) -> bool {
        self.last_height_to_be_migrated.is_some()
    }

    pub fn get_migration_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }
}

impl<Description> MigrationState<Description>
where
    Description: DatabaseDescription,
{
    pub fn add_migration_changes(&mut self, changes: Changes) {
        debug_assert!(changes.iter().all(|(column, _)| {
            *column == Column::<Description>::HistoryColumn.id()
                || *column == Column::<Description>::HistoryV2Column.id()
        }));
        let memory_store = MemoryStore::<Description>::default();
        let base_changes = std::mem::take(&mut self.changes);
        let base_transaction = StorageTransaction::transaction(
            &memory_store,
            ConflictPolicy::Overwrite,
            base_changes,
        );
        let new_changes_transaction = StorageTransaction::transaction(
            base_transaction,
            ConflictPolicy::Overwrite,
            changes,
        );

        let committed_transaction = new_changes_transaction
            .commit()
            .expect("Transaction with Overwrite conflict policy cannot fail");

        // Revert the changes above the last migration height.
        // TODO: This is a hack which depends from the implementation of `<StructuredStorage as StorageMutate<_>>`.
        // We cannot avoid filtering changes manually because iterators for `InMemoryStorage` are not implemented.
        let mut changes = committed_transaction.into_changes();
        if let Err(_) = self.remove_stale_migration_changes(&mut changes) {
            // Something went wrong, we should throw away the changes as they might contain stale data
            // and we cannot proceed with the migration.
            return
        }

        self.changes = changes;
    }

    // Remove the changes above the last migration height.
    // TODO: This is a hack which depends from the implementation of `<StructuredStorage as StorageMutate<_>>`.
    // We cannot avoid filtering changes manually because iterators for `InMemoryStorage` are not implemented.
    fn remove_stale_migration_changes(&self, changes: &mut Changes) -> StorageResult<()> {
        let mut v1_changes: Vec<ReferenceBytesKey> = Vec::new();
        let mut v2_changes: Vec<ReferenceBytesKey> = Vec::new();

        for (column, inner_changes) in changes.iter() {
            if *column == Column::<Description>::HistoryColumn.id() {
                for (serialized_height, _) in inner_changes.iter() {
                    let height: u64 = <<ModificationsHistoryV1::<Description> as TableWithBlueprint>::Blueprint as BlueprintInspect<ModificationsHistoryV1<Description>, StructuredStorage<InMemoryTransaction<InMemoryStorage<Column<Description>>>>>>::KeyCodec::decode(serialized_height).expect("Decoding an encoded value cannot fail");
                    if height > self.last_height_to_be_migrated.unwrap() {
                        v1_changes.push(serialized_height.clone());
                    }
                }
            } else if *column == Column::<Description>::HistoryV2Column.id() {
                for (serialized_height, _) in inner_changes.iter() {
                    let height: u64 = <<ModificationsHistoryV2::<Description> as TableWithBlueprint>::Blueprint as BlueprintInspect<ModificationsHistoryV1<Description>, StructuredStorage<InMemoryTransaction<InMemoryStorage<Column<Description>>>>>>::KeyCodec::decode(serialized_height).expect("Decoding an encoded value cannot fail");
                    if height > self.last_height_to_be_migrated.unwrap() {
                        v2_changes.push(serialized_height.clone());
                    }
                }
            }
        }

        for (column, inner_changes) in changes.iter_mut() {
            if *column == Column::<Description>::HistoryColumn.id() {
                inner_changes.retain(|serialized_height, _| {
                    v1_changes.contains(&serialized_height)
                });
            } else if *column == Column::<Description>::HistoryV2Column.id() {
                inner_changes.retain(|serialized_height, _| {
                    v2_changes.contains(&serialized_height)
                });
            }
        }

        Ok(())
    }
}
