use crate::{
    database::{
        Column,
        Database,
    },
    multikey,
    state::{
        Error,
        IterDirection,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_vm::{
            crypto,
            prelude::{
                Bytes32,
                ContractId,
                MerkleRoot,
                MerkleRootStorage,
            },
        },
    },
    db::ContractsState,
};
use itertools::Itertools;
use std::borrow::Cow;

impl StorageInspect<ContractsState<'_>> for Database {
    type Error = Error;

    fn get(&self, key: &(&ContractId, &Bytes32)) -> Result<Option<Cow<Bytes32>>, Error> {
        self._get(
            &multikey!(key.0, ContractId, key.1, Bytes32),
            Column::ContractsState,
        )
        .map_err(Into::into)
    }

    fn contains_key(&self, key: &(&ContractId, &Bytes32)) -> Result<bool, Error> {
        self._contains_key(
            &multikey!(key.0, ContractId, key.1, Bytes32),
            Column::ContractsState,
        )
        .map_err(Into::into)
    }
}

impl StorageMutate<ContractsState<'_>> for Database {
    fn insert(
        &mut self,
        key: &(&ContractId, &Bytes32),
        value: &Bytes32,
    ) -> Result<Option<Bytes32>, Error> {
        self._insert(
            &multikey!(key.0, ContractId, key.1, Bytes32),
            Column::ContractsState,
            value,
        )
        .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &(&ContractId, &Bytes32),
    ) -> Result<Option<Bytes32>, Error> {
        self._remove(
            &multikey!(key.0, ContractId, key.1, Bytes32),
            Column::ContractsState,
        )
        .map_err(Into::into)
    }
}

impl MerkleRootStorage<ContractId, ContractsState<'_>> for Database {
    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Error> {
        let items: Vec<_> = self
            .iter_all::<Vec<u8>, Bytes32>(
                Column::ContractsState,
                Some(parent.to_vec()),
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
    use fuel_core_interfaces::common::fuel_storage::StorageAsMut;

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
