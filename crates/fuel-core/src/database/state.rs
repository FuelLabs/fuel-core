use crate::{
    database::{
        Column,
        Database,
    },
    state::{
        IterDirection,
        MultiKey,
    },
};
use fuel_core_storage::{
    tables::ContractsState,
    Error as StorageError,
    MerkleRoot,
    MerkleRootStorage,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_types::{
        Bytes32,
        ContractId,
    },
    fuel_vm::crypto,
};
use itertools::Itertools;
use std::borrow::Cow;

impl StorageInspect<ContractsState<'_>> for Database {
    type Error = StorageError;

    fn get(
        &self,
        key: &(&ContractId, &Bytes32),
    ) -> Result<Option<Cow<Bytes32>>, Self::Error> {
        let key = MultiKey::new(key);
        self.get(key.as_ref(), Column::ContractsState)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &(&ContractId, &Bytes32)) -> Result<bool, Self::Error> {
        let key = MultiKey::new(key);
        self.exists(key.as_ref(), Column::ContractsState)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsState<'_>> for Database {
    fn insert(
        &mut self,
        key: &(&ContractId, &Bytes32),
        value: &Bytes32,
    ) -> Result<Option<Bytes32>, Self::Error> {
        let key = MultiKey::new(key);
        Database::insert(self, key.as_ref(), Column::ContractsState, *value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &(&ContractId, &Bytes32),
    ) -> Result<Option<Bytes32>, Self::Error> {
        let key = MultiKey::new(key);
        Database::remove(self, key.as_ref(), Column::ContractsState).map_err(Into::into)
    }
}

impl MerkleRootStorage<ContractId, ContractsState<'_>> for Database {
    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Self::Error> {
        let items: Vec<_> = Database::iter_all::<Vec<u8>, Bytes32>(
            self,
            Column::ContractsState,
            Some(parent.as_ref().to_vec()),
            None,
            Some(IterDirection::Forward),
        )
        .try_collect()?;

        let root = items
            .iter()
            .filter_map(|(key, value)| {
                (&key[..parent.len()] == parent.as_ref()).then_some((key, value))
            })
            .sorted_by_key(|t| t.0)
            .map(|(_, value)| value);

        Ok(crypto::ephemeral_merkle_root(root).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::StorageAsMut;

    #[test]
    fn get() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);
        let key = &(&storage_id.0, &storage_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(key, &stored_value)
            .unwrap();

        assert_eq!(
            *database
                .storage::<ContractsState>()
                .get(key)
                .unwrap()
                .unwrap(),
            stored_value
        );
    }

    #[test]
    fn put() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);
        let key = &(&storage_id.0, &storage_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(key, &stored_value)
            .unwrap();

        let returned: Bytes32 = *database
            .storage::<ContractsState>()
            .get(key)
            .unwrap()
            .unwrap();
        assert_eq!(returned, stored_value);
    }

    #[test]
    fn remove() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);
        let key = &(&storage_id.0, &storage_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(key, &stored_value)
            .unwrap();

        database.storage::<ContractsState>().remove(key).unwrap();

        assert!(!database
            .storage::<ContractsState>()
            .contains_key(key)
            .unwrap());
    }

    #[test]
    fn exists() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);
        let key = &(&storage_id.0, &storage_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(key, &stored_value)
            .unwrap();

        assert!(database
            .storage::<ContractsState>()
            .contains_key(key)
            .unwrap());
    }

    #[test]
    fn root() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);
        let key = &(&storage_id.0, &storage_id.1);

        let database = &mut Database::default();

        database
            .storage::<ContractsState>()
            .insert(key, &stored_value)
            .unwrap();

        let root = database.storage::<ContractsState>().root(&storage_id.0);
        assert!(root.is_ok())
    }
}
