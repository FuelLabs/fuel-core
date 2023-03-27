use crate::database::{
    storage::{
        ContractsAssetsMerkleData,
        ContractsAssetsMerkleMetadata,
        SparseMerkleMetadata,
    },
    Column,
    Database,
};
use fuel_core_storage::{
    tables::ContractsAssets,
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_merkle::sparse::{
        in_memory,
        MerkleTree,
    },
    fuel_types::ContractId,
};
use std::{
    borrow::{
        BorrowMut,
        Cow,
    },
    ops::Deref,
};

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
        let prev = Database::insert(self, key.as_ref(), Column::ContractsAssets, value)
            .map_err(Into::into);

        // Get latest metadata entry for this contract id
        let prev_metadata = self
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(key.contract_id())?
            .unwrap_or_default();

        let root = prev_metadata.root;
        let storage = self.borrow_mut();
        let mut tree: MerkleTree<ContractsAssetsMerkleData, _> = {
            if root == [0; 32] {
                // The tree is empty
                MerkleTree::new(storage)
            } else {
                // Load the tree saved in metadata
                MerkleTree::load(storage, &root)
                    .map_err(|err| StorageError::Other(err.into()))?
            }
        };

        // Update the contact's key-value dataset. The key is the asset id and the
        // value the Word
        tree.update(key.asset_id().deref(), value.to_be_bytes().as_slice())
            .map_err(|err| StorageError::Other(err.into()))?;

        // Generate new metadata for the updated tree
        let root = tree.root();
        let metadata = SparseMerkleMetadata { root };
        self.storage::<ContractsAssetsMerkleMetadata>()
            .insert(key.contract_id(), &metadata)?;

        prev
    }

    fn remove(
        &mut self,
        key: &<ContractsAssets as Mappable>::Key,
    ) -> Result<Option<<ContractsAssets as Mappable>::OwnedValue>, Self::Error> {
        let prev = Database::remove(self, key.as_ref(), Column::ContractsAssets)
            .map_err(Into::into);

        // Get latest metadata entry for this contract id
        let prev_metadata = self
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(key.contract_id())?;

        if let Some(prev_metadata) = prev_metadata {
            let root = prev_metadata.root;

            // Load the tree saved in metadata
            let storage = self.borrow_mut();
            let mut tree: MerkleTree<ContractsAssetsMerkleData, _> =
                MerkleTree::load(storage, &root)
                    .map_err(|err| StorageError::Other(err.into()))?;

            // Update the contract's key-value dataset. The key is the asset id and
            // the value is the Word
            tree.delete(key.asset_id().deref())
                .map_err(|err| StorageError::Other(err.into()))?;

            let root = tree.root();
            if root == in_memory::MerkleTree::new().root() {
                // The tree is now empty; remove the metadata
                self.storage::<ContractsAssetsMerkleMetadata>()
                    .remove(key.contract_id())?;
            } else {
                // Generate new metadata for the updated tree
                let metadata = SparseMerkleMetadata { root };
                self.storage::<ContractsAssetsMerkleMetadata>()
                    .insert(key.contract_id(), &metadata)?;
            }
        }

        prev
    }
}

impl MerkleRootStorage<ContractId, ContractsAssets> for Database {
    fn root(&self, parent: &ContractId) -> Result<MerkleRoot, Self::Error> {
        let metadata = self
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(parent)?;
        let root = metadata
            .map(|metadata| metadata.root)
            .unwrap_or_else(|| in_memory::MerkleTree::new().root());
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        StorageAsMut,
        StorageAsRef,
    };
    use fuel_core_types::fuel_types::{
        AssetId,
        Word,
    };

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

    #[test]
    fn root_returns_empty_root_for_invalid_contract() {
        let invalid_contract_id = ContractId::from([1u8; 32]);
        let database = Database::default();
        let empty_root = in_memory::MerkleTree::new().root();
        let root = database
            .storage::<ContractsAssets>()
            .root(&invalid_contract_id)
            .unwrap();
        assert_eq!(root, empty_root)
    }

    #[test]
    fn put_updates_the_assets_merkle_root_for_the_given_contract() {
        let contract_id = ContractId::from([1u8; 32]);
        let database = &mut Database::default();

        // Write the first contract asset
        let asset_id = AssetId::new([1u8; 32]);
        let key = (&contract_id, &asset_id).into();
        let balance: Word = 100;
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        // Read the first Merkle root
        let root_1 = database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .unwrap();

        // Write the second contract asset
        let asset_id = AssetId::new([2u8; 32]);
        let key = (&contract_id, &asset_id).into();
        let balance: Word = 100;
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        // Read the second Merkle root
        let root_2 = database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .unwrap();

        assert_ne!(root_1, root_2);
    }

    #[test]
    fn put_creates_merkle_metadata_when_empty() {
        let contract_id = ContractId::from([1u8; 32]);
        let asset_id = AssetId::new([1u8; 32]);
        let key = (&contract_id, &asset_id).into();
        let database = &mut Database::default();

        // Write a contract asset
        let balance: Word = 100;
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        // Read the Merkle metadata
        let metadata = database
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(&contract_id)
            .unwrap();

        assert!(metadata.is_some());
    }

    #[test]
    fn remove_updates_the_assets_merkle_root_for_the_given_contract() {
        let contract_id = ContractId::from([1u8; 32]);
        let database = &mut Database::default();

        // Write the first contract asset
        let asset_id = AssetId::new([1u8; 32]);
        let key = (&contract_id, &asset_id).into();
        let balance: Word = 100;
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();
        let root_0 = database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .unwrap();

        // Write the second contract asset
        let asset_id = AssetId::new([2u8; 32]);
        let key = (&contract_id, &asset_id).into();
        let balance: Word = 100;
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        // Read the first Merkle root
        let root_1 = database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .unwrap();

        // Remove the first contract asset
        let asset_id = AssetId::new([2u8; 32]);
        let key = (&contract_id, &asset_id).into();
        database.storage::<ContractsAssets>().remove(&key).unwrap();

        // Read the second Merkle root
        let root_2 = database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .unwrap();

        assert_ne!(root_1, root_2);
        assert_eq!(root_0, root_2);
    }

    #[test]
    fn remove_deletes_merkle_metadata_when_empty() {
        let contract_id = ContractId::from([1u8; 32]);
        let asset_id = AssetId::new([1u8; 32]);
        let key = (&contract_id, &asset_id).into();
        let database = &mut Database::default();

        // Write a contract asset
        let balance: Word = 100;
        database
            .storage::<ContractsAssets>()
            .insert(&key, &balance)
            .unwrap();

        // Read the Merkle metadata
        database
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(&contract_id)
            .unwrap()
            .expect("Expected Merkle metadata to be present");

        // Remove the contract asset
        database.storage::<ContractsAssets>().remove(&key).unwrap();

        // Read the Merkle metadata
        let metadata = database
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(&contract_id)
            .unwrap();

        assert!(metadata.is_none());
    }
}
