use crate::{
    database::{
        Column,
        Database,
    },
    state::{
        Error,
        IterDirection,
        MultiKey,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            MerkleRoot,
            StorageInspect,
            StorageMutate,
        },
        fuel_vm::{
            crypto,
            prelude::{
                AssetId,
                ContractId,
                MerkleRootStorage,
                Word,
            },
        },
    },
    db::ContractsAssets,
};
use itertools::Itertools;
use std::borrow::Cow;

impl StorageInspect<ContractsAssets<'_>> for Database {
    type Error = Error;

    fn get(&self, key: &(&ContractId, &AssetId)) -> Result<Option<Cow<Word>>, Error> {
        let key = MultiKey::new(key);
        self.get(key.as_ref(), Column::ContractsAssets)
    }

    fn contains_key(&self, key: &(&ContractId, &AssetId)) -> Result<bool, Error> {
        let key = MultiKey::new(key);
        self.exists(key.as_ref(), Column::ContractsAssets)
    }
}

impl StorageMutate<ContractsAssets<'_>> for Database {
    fn insert(
        &mut self,
        key: &(&ContractId, &AssetId),
        value: &Word,
    ) -> Result<Option<Word>, Error> {
        let key = MultiKey::new(key);
        Database::insert(self, key.as_ref(), Column::ContractsAssets, *value)
    }

    fn remove(&mut self, key: &(&ContractId, &AssetId)) -> Result<Option<Word>, Error> {
        let key = MultiKey::new(key);
        Database::remove(self, key.as_ref(), Column::ContractsAssets)
    }
}

impl MerkleRootStorage<ContractId, ContractsAssets<'_>> for Database {
    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Error> {
        let items: Vec<_> = Database::iter_all::<Vec<u8>, Word>(
            self,
            Column::ContractsAssets,
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
            .map(|(_, value)| value.to_be_bytes());

        Ok(crypto::ephemeral_merkle_root(root).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_interfaces::common::fuel_storage::StorageAsMut;

    #[test]
    fn get() {
        let balance_id: (ContractId, AssetId) =
            (ContractId::from([1u8; 32]), AssetId::new([1u8; 32]));
        let balance: Word = 100;
        let key = &(&balance_id.0, &balance_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(key, &balance)
            .unwrap();

        assert_eq!(
            database
                .storage::<ContractsAssets>()
                .get(key)
                .unwrap()
                .unwrap()
                .into_owned(),
            balance
        );
    }

    #[test]
    fn put() {
        let balance_id: (ContractId, AssetId) =
            (ContractId::from([1u8; 32]), AssetId::new([1u8; 32]));
        let balance: Word = 100;
        let key = &(&balance_id.0, &balance_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(key, &balance)
            .unwrap();

        let returned = database
            .storage::<ContractsAssets>()
            .get(key)
            .unwrap()
            .unwrap();
        assert_eq!(*returned, balance);
    }

    #[test]
    fn remove() {
        let balance_id: (ContractId, AssetId) =
            (ContractId::from([1u8; 32]), AssetId::new([1u8; 32]));
        let balance: Word = 100;
        let key = &(&balance_id.0, &balance_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(key, &balance)
            .unwrap();

        database.storage::<ContractsAssets>().remove(key).unwrap();

        assert!(!database
            .storage::<ContractsAssets>()
            .contains_key(key)
            .unwrap());
    }

    #[test]
    fn exists() {
        let balance_id: (ContractId, AssetId) =
            (ContractId::from([1u8; 32]), AssetId::new([1u8; 32]));
        let balance: Word = 100;
        let key = &(&balance_id.0, &balance_id.1);

        let database = &mut Database::default();
        database
            .storage::<ContractsAssets>()
            .insert(key, &balance)
            .unwrap();

        assert!(database
            .storage::<ContractsAssets>()
            .contains_key(key)
            .unwrap());
    }

    #[test]
    fn root() {
        let balance_id: (ContractId, AssetId) =
            (ContractId::from([1u8; 32]), AssetId::new([1u8; 32]));
        let balance: Word = 100;
        let key = &(&balance_id.0, &balance_id.1);

        let database = &mut Database::default();

        database
            .storage::<ContractsAssets>()
            .insert(key, &balance)
            .unwrap();

        let root = database.storage::<ContractsAssets>().root(&balance_id.0);
        assert!(root.is_ok())
    }
}
