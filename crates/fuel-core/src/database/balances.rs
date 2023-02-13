use crate::database::{
    Column,
    Database,
};
use fuel_core_storage::{
    iter::IterDirection,
    tables::ContractsAssets,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::ContractId,
    fuel_vm::crypto,
};
use itertools::Itertools;
use std::borrow::Cow;

impl StorageInspect<ContractsAssets> for Database {
    type Error = StorageError;

    fn get(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<Cow<<ContractsAssets as Mappable>::OwnedValue>>, Self::Error> {
        self.get(key.as_ref(), Column::ContractsAssets)
            .map_err(Into::into)
    }

    fn contains_key(
        &self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<bool, Self::Error> {
        self.contains_key(key.as_ref(), Column::ContractsAssets)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsAssets> for Database {
    fn insert(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
        value: &<ContractsAssets as Mappable>::Value,
    ) -> Result<Option<<ContractsAssets as Mappable>::OwnedValue>, Self::Error> {
        Database::insert(self, key.as_ref(), Column::ContractsAssets, value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<<ContractsAssets as Mappable>::OwnedValue>, Self::Error> {
        Database::remove(self, key.as_ref(), Column::ContractsAssets).map_err(Into::into)
    }
}

impl MerkleRootStorage<ContractId, ContractsAssets> for Database {
    fn root(&self, parent: &ContractId) -> Result<MerkleRoot, Self::Error> {
        let items: Vec<(Vec<u8>, Word)> = Database::iter_all_by_prefix(
            self,
            Column::ContractsAssets,
            Some(parent),
            Some(IterDirection::Forward),
        )
        .try_collect()?;

        let root = items
            .iter()
            .filter_map(|(key, value)| {
                (&key[..parent.len()] == parent.as_ref()).then_some((key, value))
            })
            .sorted_by_key(|t| t.0)
            .map(|(_, value)| value.to_be_bytes());

        Ok(crypto::ephemeral_merkle_root(root).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        StorageAsMut,
        StorageAsRef,
    };
    use fuel_core_types::fuel_types::AssetId;

    #[test]
    fn get() {
        let key = (&ContractId::from([1u8; 32]), &AssetId::new([1u8; 32])).into();
        let balance: Word = 100;

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        assert_eq!(
            database
                .storage::<ContractsAssets>()
                .get(&key)
                .unwrap()
                .unwrap()
                .into_owned(),
            balance
        );
    }

    #[test]
    fn put() {
        let key = (&ContractId::from([1u8; 32]), &AssetId::new([1u8; 32])).into();
        let balance: Word = 100;

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        let returned = database
            .storage::<ContractsAssets>()
            .get(&key)
            .unwrap()
            .unwrap();
        assert_eq!(*returned, balance);
    }

    #[test]
    fn remove() {
        let key = (&ContractId::from([1u8; 32]), &AssetId::new([1u8; 32])).into();
        let balance: Word = 100;

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        database.storage::<ContractsAssets>().remove(&key).unwrap();

        assert!(!database
            .storage::<ContractsAssets>()
            .contains_key(&key)
            .unwrap());
    }

    #[test]
    fn exists() {
        let key = (&ContractId::from([1u8; 32]), &AssetId::new([1u8; 32])).into();
        let balance: Word = 100;

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        assert!(database
            .storage::<ContractsAssets>()
            .contains_key(&key)
            .unwrap());
    }

    #[test]
    fn root() {
        let key = (&ContractId::from([1u8; 32]), &AssetId::new([1u8; 32])).into();
        let balance: Word = 100;

        let mut database = Database::default();

        StorageMutate::<ContractsAssets>::insert(&mut database, &key, &balance).unwrap();

        let root = database
            .storage::<ContractsAssets>()
            .root(key.contract_id());
        assert!(root.is_ok())
    }
}
