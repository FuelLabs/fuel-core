use crate::database::{columns, Database, KvStoreError};
use fuel_core_interfaces::relayer::StakingDiff;
use fuel_storage::Storage;
use std::borrow::Cow;

impl Storage<u64, StakingDiff> for Database {
    type Error = KvStoreError;

    fn insert(
        &mut self,
        key: &u64,
        value: &StakingDiff,
    ) -> Result<Option<StakingDiff>, KvStoreError> {
        Database::insert(
            self,
            key.to_be_bytes(),
            columns::STAKING_DIFFS,
            value.clone(),
        )
        .map_err(Into::into)
    }

    fn remove(&mut self, key: &u64) -> Result<Option<StakingDiff>, KvStoreError> {
        Database::remove(self, &key.to_be_bytes(), columns::STAKING_DIFFS).map_err(Into::into)
    }

    fn get(&self, key: &u64) -> Result<Option<Cow<StakingDiff>>, KvStoreError> {
        Database::get(self, &key.to_be_bytes(), columns::STAKING_DIFFS).map_err(Into::into)
    }

    fn contains_key(&self, key: &u64) -> Result<bool, KvStoreError> {
        Database::exists(self, &key.to_be_bytes(), columns::STAKING_DIFFS).map_err(Into::into)
    }
}
