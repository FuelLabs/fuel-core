use crate::database::{
    storage::{
        ContractsAssetsMerkleData,
        ContractsAssetsMerkleMetadata,
        DatabaseColumn,
        SparseMerkleMetadata,
    },
    Column,
    Database,
};
use fuel_core_storage::{
    tables::ContractsAssets,
    ContractsAssetKey,
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
    fuel_asm::Word,
    fuel_merkle::{
        sparse,
        sparse::{
            in_memory,
            MerkleTree,
            MerkleTreeKey,
        },
    },
    fuel_types::{
        AssetId,
        ContractId,
    },
};
use itertools::Itertools;
use std::borrow::{
    BorrowMut,
    Cow,
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
        let mut tree: MerkleTree<ContractsAssetsMerkleData, _> =
            MerkleTree::load(storage, &root)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Update the contact's key-value dataset. The key is the asset id and the
        // value the Word
        tree.update(MerkleTreeKey::new(key), value.to_be_bytes().as_slice())
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

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
        let prev = Database::take(self, key.as_ref(), Column::ContractsAssets)
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
                    .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            // Update the contract's key-value dataset. The key is the asset id and
            // the value is the Word
            tree.delete(MerkleTreeKey::new(key))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            let root = tree.root();
            if root == *sparse::empty_sum() {
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

impl Database {
    /// Initialize the balances of the contract from the all leafs.
    /// This method is more performant than inserting balances one by one.
    pub fn init_contract_balances<S>(
        &mut self,
        contract_id: &ContractId,
        balances: S,
    ) -> Result<(), StorageError>
    where
        S: Iterator<Item = (AssetId, Word)>,
    {
        if self
            .storage::<ContractsAssetsMerkleMetadata>()
            .contains_key(contract_id)?
        {
            return Err(
                anyhow::anyhow!("The contract balances is already initialized").into(),
            )
        }

        let balances = balances.collect_vec();

        // Keys and values should be original without any modifications.
        // Key is `ContractId` ++ `AssetId`
        self.batch_insert(
            Column::ContractsAssets,
            balances.clone().into_iter().map(|(asset, value)| {
                (ContractsAssetKey::new(contract_id, &asset), value)
            }),
        )?;

        // Merkle data:
        // - Asset key should be converted into `MerkleTreeKey` by `new` function that hashes them.
        // - The balance value are original.
        let balances = balances.into_iter().map(|(asset, value)| {
            (
                MerkleTreeKey::new(ContractsAssetKey::new(contract_id, &asset)),
                value.to_be_bytes(),
            )
        });
        let (root, nodes) = in_memory::MerkleTree::nodes_from_set(balances);
        self.batch_insert(ContractsAssetsMerkleData::column(), nodes.into_iter())?;
        let metadata = SparseMerkleMetadata { root };
        self.storage::<ContractsAssetsMerkleMetadata>()
            .insert(contract_id, &metadata)?;

        Ok(())
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
    use rand::Rng;

    fn random_asset_id<R>(rng: &mut R) -> AssetId
    where
        R: Rng + ?Sized,
    {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        bytes.into()
    }

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
    fn updating_foreign_contract_does_not_affect_the_given_contract_insertion() {
        let given_contract_id = ContractId::from([1u8; 32]);
        let foreign_contract_id = ContractId::from([2u8; 32]);
        let database = &mut Database::default();

        let asset_id = AssetId::new([0u8; 32]);
        let balance: Word = 100;

        // Given
        let given_contract_key = (&given_contract_id, &asset_id).into();
        let foreign_contract_key = (&foreign_contract_id, &asset_id).into();
        database
            .storage::<ContractsAssets>()
            .insert(&given_contract_key, &balance)
            .unwrap();

        // When
        database
            .storage::<ContractsAssets>()
            .insert(&foreign_contract_key, &balance)
            .unwrap();
        database
            .storage::<ContractsAssets>()
            .remove(&foreign_contract_key)
            .unwrap();

        // Then
        let result = database
            .storage::<ContractsAssets>()
            .insert(&given_contract_key, &balance)
            .unwrap();

        assert!(result.is_some());
    }

    #[test]
    fn init_contract_balances_works() {
        use rand::{
            rngs::StdRng,
            RngCore,
            SeedableRng,
        };

        let rng = &mut StdRng::seed_from_u64(1234);
        let gen = || Some((random_asset_id(rng), rng.next_u64()));
        let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();

        let contract_id = ContractId::from([1u8; 32]);
        let init_database = &mut Database::default();

        init_database
            .init_contract_balances(&contract_id, data.clone().into_iter())
            .expect("Should init contract");
        let init_root = init_database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .expect("Should get root");

        let seq_database = &mut Database::default();
        for (asset, value) in data.iter() {
            seq_database
                .storage::<ContractsAssets>()
                .insert(&ContractsAssetKey::new(&contract_id, asset), value)
                .expect("Should insert a state");
        }
        let seq_root = seq_database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .expect("Should get root");

        assert_eq!(init_root, seq_root);

        for (asset, value) in data.into_iter() {
            let init_value = init_database
                .storage::<ContractsAssets>()
                .get(&ContractsAssetKey::new(&contract_id, &asset))
                .expect("Should get a state from init database")
                .unwrap()
                .into_owned();
            let seq_value = seq_database
                .storage::<ContractsAssets>()
                .get(&ContractsAssetKey::new(&contract_id, &asset))
                .expect("Should get a state from seq database")
                .unwrap()
                .into_owned();
            assert_eq!(init_value, value);
            assert_eq!(seq_value, value);
        }
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
