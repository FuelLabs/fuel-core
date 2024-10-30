use fuel_core_storage::transactional::Changes;

pub struct MigrationState {
    _changes: Changes,
}

impl MigrationState {
    pub fn new() -> Self {
        Self {
            _changes: Changes::default(),
        }
    }

    pub fn is_migration_in_progress(&self) -> bool {
        true
    }
}
