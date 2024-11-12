use std::marker::PhantomData;

use fuel_core_storage::{
    blueprint::BlueprintInspect,
    iter::{
        changes_iterator::ChangesIterator,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        StorageColumn,
        WriteOperation,
    },
    structured_storage::{
        test::InMemoryStorage,
        StructuredStorage,
        TableWithBlueprint,
    },
    transactional::{
        Changes,
        ConflictPolicy,
        InMemoryTransaction,
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
        StorageResult,
    },
};
use fuel_core_storage::codec::Decode;

enum MigrationStatus {
    InProgress { last_height_to_be_migrated: u64 },
    Completed,
}

pub struct MigrationState<Description> {
    changes: Changes,
    // The height up to which the migration can be performed, included.
    status: MigrationStatus,
    _description: PhantomData<Description>,
}

impl<Description> Default for MigrationState<Description> {
    fn default() -> Self {
        Self {
            changes: Changes::default(),
            status: MigrationStatus::InProgress {
                last_height_to_be_migrated: u64::MAX,
            },
            _description: PhantomData,
        }
    }
}

impl<Description> MigrationState<Description> {
    pub fn is_migration_in_progress(&self) -> bool {
        matches!(self.status, MigrationStatus::InProgress { .. })
    }

    pub fn complete_migration(&mut self) {
        self.status = MigrationStatus::Completed;
    }

    pub fn take_migration_changes(&mut self) -> Option<Changes> {
        (self.is_migration_in_progress()).then(|| std::mem::take(&mut self.changes))
    }
}

impl<Description> MigrationState<Description>
where
    Description: DatabaseDescription,
{
    pub fn add_migration_changes(&mut self, changes: Changes) {
        debug_assert!(changes.keys().all(|column| {
            *column == Column::<Description>::HistoryColumn.id()
                || *column == Column::<Description>::HistoryV2Column.id()
        }));
        let memory_store = InMemoryStorage::<Description::Column>::default();
        // TODO: Consider cloning the changes instead of moving them, to avoid losing consistent changes in case of error.
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
        let changes = committed_transaction.into_changes();
        let Ok(consistent_changes) = self.remove_stale_migration_changes(changes) else {
            // Something went wrong, we should throw away the changes as they might contain stale data
            // and we cannot proceed with the migration.
            return
        };

        self.changes = consistent_changes;
    }

    // Remove the changes above the last migration height.
    fn remove_stale_migration_changes(&self, changes: Changes) -> StorageResult<Changes> {
        let last_height_to_be_migrated = match self.status {
            MigrationStatus::InProgress {
                last_height_to_be_migrated,
            } => last_height_to_be_migrated,
            MigrationStatus::Completed => return Ok(Changes::default()),
        };

        let mut revert_changes = Changes::default();
        revert_changes.insert(
            Column::<Description>::HistoryV2Column.id(),
            Default::default(),
        );

        // Changes_iterator iterates over keys for which the corresponding change is a
        let changes_iterator = ChangesIterator::new(&changes);
        for serialized_height in changes_iterator.iter_store_keys(
            Column::<Description>::HistoryV2Column,
            None,
            None,
            IterDirection::Forward,
        ) {
            let serialized_height = serialized_height?;
            let height: u64 = <
                        <ModificationsHistoryV2::<Description> as TableWithBlueprint>
                            ::Blueprint as BlueprintInspect<
                                ModificationsHistoryV1<Description>,
                                StructuredStorage<
                                    InMemoryTransaction<
                                        InMemoryStorage<Column<Description>>
                                    >
                                >
                            >
                        >::KeyCodec::decode(&serialized_height)?;
            if height > last_height_to_be_migrated {
                revert_changes
                    .get_mut(&Column::<Description>::HistoryV2Column.id())
                    .expect("Changes for HistoryV2Column were inserted in this function")
                    .insert(serialized_height.into(), WriteOperation::Remove);
            }
        }

        let base_transaction = StorageTransaction::transaction(
            InMemoryStorage::<Column<Description>>::default(),
            ConflictPolicy::Overwrite,
            changes,
        );

        let revert_stale_changes_transaction = StorageTransaction::transaction(
            base_transaction,
            ConflictPolicy::Overwrite,
            revert_changes,
        );

        let transaction_without_stale_changes = revert_stale_changes_transaction
            .commit()
            .expect("Transaction with Overwrite conflict policy cannot fail");

        Ok(transaction_without_stale_changes.into_changes())
    }

    pub fn update_last_height_to_be_migrated(&mut self, last_height_to_be_migrated: u64) {
        match self.status {
            MigrationStatus::InProgress {
                last_height_to_be_migrated: current_last_height_to_be_migrated,
            } => {
                self.status = MigrationStatus::InProgress {
                    last_height_to_be_migrated: last_height_to_be_migrated
                        .min(current_last_height_to_be_migrated),
                };
            }
            MigrationStatus::Completed => {}
        }
    }
}
