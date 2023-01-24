use crate::{
    database::{
        Column,
        Database,
    },
    state::IterDirection,
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
    fuel_merkle::storage::Mappable,
    fuel_types::{
        Bytes32,
        ContractId,
    },
    fuel_vm::crypto,
};
use itertools::Itertools;
use std::borrow::Cow;

impl StorageInspect<ContractsState> for Database {
    type Error = StorageError;

    fn get(
        &self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<Cow<<ContractsState as Mappable>::OwnedValue>>, Self::Error> {
        self.get(key.as_ref(), Column::ContractsState)
            .map_err(Into::into)
    }

    fn contains_key(
        &self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<bool, Self::Error> {
        self.exists(key.as_ref(), Column::ContractsState)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsState> for Database {
    fn insert(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
        value: &<ContractsState as Mappable>::Value,
    ) -> Result<Option<<ContractsState as Mappable>::OwnedValue>, Self::Error> {
        Database::insert(self, key.as_ref(), Column::ContractsState, *value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<<ContractsState as Mappable>::OwnedValue>, Self::Error> {
        Database::remove(self, key.as_ref(), Column::ContractsState).map_err(Into::into)
    }
}

impl MerkleRootStorage<ContractId, ContractsState> for Database {
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
        let key = (&ContractId::from([1u8; 32]), &Bytes32::from([1u8; 32])).into();
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(&key, &stored_value)
            .unwrap();

        assert_eq!(
            *database
                .storage::<ContractsState>()
                .get(&key)
                .unwrap()
                .unwrap(),
            stored_value
        );
    }

    #[test]
    fn put() {
        let key = (&ContractId::from([1u8; 32]), &Bytes32::from([1u8; 32])).into();
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(&key, &stored_value)
            .unwrap();

        let returned: Bytes32 = *database
            .storage::<ContractsState>()
            .get(&key)
            .unwrap()
            .unwrap();
        assert_eq!(returned, stored_value);
    }

    #[test]
    fn remove() {
        let key = (&ContractId::from([1u8; 32]), &Bytes32::from([1u8; 32])).into();
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(&key, &stored_value)
            .unwrap();

        database.storage::<ContractsState>().remove(&key).unwrap();

        assert!(!database
            .storage::<ContractsState>()
            .contains_key(&key)
            .unwrap());
    }

    #[test]
    fn exists() {
        let key = (&ContractId::from([1u8; 32]), &Bytes32::from([1u8; 32])).into();
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = &mut Database::default();
        database
            .storage::<ContractsState>()
            .insert(&key, &stored_value)
            .unwrap();

        assert!(database
            .storage::<ContractsState>()
            .contains_key(&key)
            .unwrap());
    }

    #[test]
    fn root() {
        let key = (&ContractId::from([1u8; 32]), &Bytes32::from([1u8; 32])).into();
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = &mut Database::default();

        database
            .storage::<ContractsState>()
            .insert(&key, &stored_value)
            .unwrap();

        let root = database.storage::<ContractsState>().root(key.contract_id());
        assert!(root.is_ok())
    }
}
