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
    tables::ContractsAssets,
    Error as StorageError,
    MerkleRoot,
    MerkleRootStorage,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::{
        AssetId,
        ContractId,
    },
    fuel_vm::crypto,
};
use itertools::Itertools;
use std::borrow::Cow;

impl StorageInspect<ContractsAssets<'_>> for Database {
    type Error = StorageError;

    fn get(
        &self,
        key: &(&ContractId, &AssetId),
    ) -> Result<Option<Cow<Word>>, Self::Error> {
        let key = MultiKey::new(key);
        self.get(key.as_ref(), Column::ContractsAssets)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &(&ContractId, &AssetId)) -> Result<bool, Self::Error> {
        let key = MultiKey::new(key);
        self.exists(key.as_ref(), Column::ContractsAssets)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsAssets<'_>> for Database {
    fn insert(
        &mut self,
        key: &(&ContractId, &AssetId),
        value: &Word,
    ) -> Result<Option<Word>, Self::Error> {
        let key = MultiKey::new(key);
        Database::insert(self, key.as_ref(), Column::ContractsAssets, *value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        key: &(&ContractId, &AssetId),
    ) -> Result<Option<Word>, Self::Error> {
        let key = MultiKey::new(key);
        Database::remove(self, key.as_ref(), Column::ContractsAssets).map_err(Into::into)
    }
}

impl MerkleRootStorage<ContractId, ContractsAssets<'_>> for Database {
    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Self::Error> {
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
    use fuel_core_storage::StorageAsMut;

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
