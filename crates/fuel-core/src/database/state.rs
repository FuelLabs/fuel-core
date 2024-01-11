use crate::database::{
    storage::{
        ContractsStateMerkleData,
        ContractsStateMerkleMetadata,
        DatabaseColumn,
        SparseMerkleMetadata,
    },
    Column,
    Database,
};
use fuel_core_storage::{
    tables::ContractsState,
    ContractsStateKey,
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
    fuel_merkle::{
        sparse,
        sparse::{
            in_memory,
            MerkleTree,
            MerkleTreeKey,
        },
    },
    fuel_types::{
        Bytes32,
        ContractId,
    },
};
use itertools::Itertools;
use std::borrow::{
    BorrowMut,
    Cow,
};

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
        self.contains_key(key.as_ref(), Column::ContractsState)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsState> for Database {
    fn insert(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
        value: &<ContractsState as Mappable>::Value,
    ) -> Result<Option<<ContractsState as Mappable>::OwnedValue>, Self::Error> {
        let prev = Database::insert(self, key.as_ref(), Column::ContractsState, value)
            .map_err(Into::into);

        // Get latest metadata entry for this contract id
        let prev_metadata = self
            .storage::<ContractsStateMerkleMetadata>()
            .get(key.contract_id())?
            .unwrap_or_default();

        let root = prev_metadata.root;
        let storage = self.borrow_mut();
        let mut tree: MerkleTree<ContractsStateMerkleData, _> =
            MerkleTree::load(storage, &root)
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Update the contract's key-value dataset. The key is the state key and
        // the value is the 32 bytes
        tree.update(MerkleTreeKey::new(key), value.as_slice())
            .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

        // Generate new metadata for the updated tree
        let root = tree.root();
        let metadata = SparseMerkleMetadata { root };
        self.storage::<ContractsStateMerkleMetadata>()
            .insert(key.contract_id(), &metadata)?;

        prev
    }

    fn remove(
        &mut self,
        key: &<ContractsState as Mappable>::Key,
    ) -> Result<Option<<ContractsState as Mappable>::OwnedValue>, Self::Error> {
        let prev = Database::take(self, key.as_ref(), Column::ContractsState)
            .map_err(Into::into);

        // Get latest metadata entry for this contract id
        let prev_metadata = self
            .storage::<ContractsStateMerkleMetadata>()
            .get(key.contract_id())?;

        if let Some(prev_metadata) = prev_metadata {
            let root = prev_metadata.root;

            // Load the tree saved in metadata
            let storage = self.borrow_mut();
            let mut tree: MerkleTree<ContractsStateMerkleData, _> =
                MerkleTree::load(storage, &root)
                    .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            // Update the contract's key-value dataset. The key is the state key and
            // the value is the 32 bytes
            tree.delete(MerkleTreeKey::new(key))
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;

            let root = tree.root();
            if root == *sparse::empty_sum() {
                // The tree is now empty; remove the metadata
                self.storage::<ContractsStateMerkleMetadata>()
                    .remove(key.contract_id())?;
            } else {
                // Generate new metadata for the updated tree
                let metadata = SparseMerkleMetadata { root };
                self.storage::<ContractsStateMerkleMetadata>()
                    .insert(key.contract_id(), &metadata)?;
            }
        }

        prev
    }
}

impl MerkleRootStorage<ContractId, ContractsState> for Database {
    fn root(&self, parent: &ContractId) -> Result<MerkleRoot, Self::Error> {
        let metadata = self.storage::<ContractsStateMerkleMetadata>().get(parent)?;
        let root = metadata
            .map(|metadata| metadata.root)
            .unwrap_or_else(|| in_memory::MerkleTree::new().root());
        Ok(root)
    }
}

impl Database {
    /// Initialize the state of the contract from all leaves.
    /// This method is more performant than inserting state one by one.
    pub fn init_contract_state<S>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> Result<(), StorageError>
    where
        S: Iterator<Item = (Bytes32, Bytes32)>,
    {
        let slots = slots.collect_vec();

        if slots.is_empty() {
            return Ok(())
        }

        if self
            .storage::<ContractsStateMerkleMetadata>()
            .contains_key(contract_id)?
        {
            return Err(anyhow::anyhow!("The contract state is already initialized").into())
        }

        // Keys and values should be original without any modifications.
        // Key is `ContractId` ++ `StorageKey`
        self.batch_insert(
            Column::ContractsState,
            slots
                .clone()
                .into_iter()
                .map(|(key, value)| (ContractsStateKey::new(contract_id, &key), value)),
        )?;

        // Merkle data:
        // - State key should be converted into `MerkleTreeKey` by `new` function that hashes them.
        // - The state value are original.
        let slots = slots.into_iter().map(|(key, value)| {
            (
                MerkleTreeKey::new(ContractsStateKey::new(contract_id, &key)),
                value,
            )
        });
        let (root, nodes) = in_memory::MerkleTree::nodes_from_set(slots);
        self.batch_insert(ContractsStateMerkleData::column(), nodes.into_iter())?;
        let metadata = SparseMerkleMetadata { root };
        self.storage::<ContractsStateMerkleMetadata>()
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
    use fuel_core_types::fuel_types::Bytes32;
    use rand::Rng;

    fn random_bytes32<R>(rng: &mut R) -> Bytes32
    where
        R: Rng + ?Sized,
    {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        bytes.into()
    }

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

        let mut database = Database::default();

        StorageMutate::<ContractsState>::insert(&mut database, &key, &stored_value)
            .unwrap();

        let root = database.storage::<ContractsState>().root(key.contract_id());
        assert!(root.is_ok())
    }

    #[test]
    fn root_returns_empty_root_for_invalid_contract() {
        let invalid_contract_id = ContractId::from([1u8; 32]);
        let database = Database::default();
        let empty_root = in_memory::MerkleTree::new().root();
        let root = database
            .storage::<ContractsState>()
            .root(&invalid_contract_id)
            .unwrap();
        assert_eq!(root, empty_root)
    }

    #[test]
    fn put_updates_the_state_merkle_root_for_the_given_contract() {
        let contract_id = ContractId::from([1u8; 32]);
        let database = &mut Database::default();

        // Write the first contract state
        let state_key = Bytes32::from([1u8; 32]);
        let state = Bytes32::from([0xff; 32]);
        let key = (&contract_id, &state_key).into();
        database
            .storage::<ContractsState>()
            .insert(&key, &state)
            .unwrap();

        // Read the first Merkle root
        let root_1 = database
            .storage::<ContractsState>()
            .root(&contract_id)
            .unwrap();

        // Write the second contract state
        let state_key = Bytes32::from([2u8; 32]);
        let state = Bytes32::from([0xff; 32]);
        let key = (&contract_id, &state_key).into();
        database
            .storage::<ContractsState>()
            .insert(&key, &state)
            .unwrap();

        // Read the second Merkle root
        let root_2 = database
            .storage::<ContractsState>()
            .root(&contract_id)
            .unwrap();

        assert_ne!(root_1, root_2);
    }

    #[test]
    fn put_creates_merkle_metadata_when_empty() {
        let contract_id = ContractId::from([1u8; 32]);
        let state_key = Bytes32::new([1u8; 32]);
        let state = Bytes32::from([0xff; 32]);
        let key = (&contract_id, &state_key).into();
        let database = &mut Database::default();

        // Write a contract state
        database
            .storage::<ContractsState>()
            .insert(&key, &state)
            .unwrap();

        // Read the Merkle metadata
        let metadata = database
            .storage::<ContractsStateMerkleMetadata>()
            .get(&contract_id)
            .unwrap();

        assert!(metadata.is_some());
    }

    #[test]
    fn remove_updates_the_state_merkle_root_for_the_given_contract() {
        let contract_id = ContractId::from([1u8; 32]);
        let database = &mut Database::default();

        // Write the first contract state
        let state_key = Bytes32::new([1u8; 32]);
        let state = Bytes32::from([0xff; 32]);
        let key = (&contract_id, &state_key).into();
        database
            .storage::<ContractsState>()
            .insert(&key, &state)
            .unwrap();
        let root_0 = database
            .storage::<ContractsState>()
            .root(&contract_id)
            .unwrap();

        // Write the second contract state
        let state_key = Bytes32::new([2u8; 32]);
        let state = Bytes32::from([0xff; 32]);
        let key = (&contract_id, &state_key).into();
        database
            .storage::<ContractsState>()
            .insert(&key, &state)
            .unwrap();

        // Read the first Merkle root
        let root_1 = database
            .storage::<ContractsState>()
            .root(&contract_id)
            .unwrap();

        // Remove the first contract state
        let state_key = Bytes32::new([2u8; 32]);
        let key = (&contract_id, &state_key).into();
        database.storage::<ContractsState>().remove(&key).unwrap();

        // Read the second Merkle root
        let root_2 = database
            .storage::<ContractsState>()
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

        let state_key = Bytes32::new([1u8; 32]);
        let state_value = Bytes32::from([0xff; 32]);

        // Given
        let given_contract_key = (&given_contract_id, &state_key).into();
        let foreign_contract_key = (&foreign_contract_id, &state_key).into();
        database
            .storage::<ContractsState>()
            .insert(&given_contract_key, &state_value)
            .unwrap();

        // When
        database
            .storage::<ContractsState>()
            .insert(&foreign_contract_key, &state_value)
            .unwrap();
        database
            .storage::<ContractsState>()
            .remove(&foreign_contract_key)
            .unwrap();

        // Then
        let result = database
            .storage::<ContractsState>()
            .insert(&given_contract_key, &state_value)
            .unwrap();

        assert!(result.is_some());
    }

    #[test]
    fn init_contract_state_works() {
        use rand::{
            rngs::StdRng,
            SeedableRng,
        };

        let rng = &mut StdRng::seed_from_u64(1234);
        let gen = || Some((random_bytes32(rng), random_bytes32(rng)));
        let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();

        let contract_id = ContractId::from([1u8; 32]);
        let init_database = &mut Database::default();

        init_database
            .init_contract_state(&contract_id, data.clone().into_iter())
            .expect("Should init contract");
        let init_root = init_database
            .storage::<ContractsState>()
            .root(&contract_id)
            .expect("Should get root");

        let seq_database = &mut Database::default();
        for (key, value) in data.iter() {
            seq_database
                .storage::<ContractsState>()
                .insert(&ContractsStateKey::new(&contract_id, key), value)
                .expect("Should insert a state");
        }
        let seq_root = seq_database
            .storage::<ContractsState>()
            .root(&contract_id)
            .expect("Should get root");

        assert_eq!(init_root, seq_root);

        for (key, value) in data.into_iter() {
            let init_value = init_database
                .storage::<ContractsState>()
                .get(&ContractsStateKey::new(&contract_id, &key))
                .expect("Should get a state from init database")
                .unwrap()
                .into_owned();
            let seq_value = seq_database
                .storage::<ContractsState>()
                .get(&ContractsStateKey::new(&contract_id, &key))
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
        let state_key = Bytes32::new([1u8; 32]);
        let state = Bytes32::from([0xff; 32]);
        let key = (&contract_id, &state_key).into();
        let database = &mut Database::default();

        // Write a contract state
        database
            .storage::<ContractsState>()
            .insert(&key, &state)
            .unwrap();

        // Read the Merkle metadata
        database
            .storage::<ContractsStateMerkleMetadata>()
            .get(&contract_id)
            .unwrap()
            .expect("Expected Merkle metadata to be present");

        // Remove the contract asset
        database.storage::<ContractsState>().remove(&key).unwrap();

        // Read the Merkle metadata
        let metadata = database
            .storage::<ContractsStateMerkleMetadata>()
            .get(&contract_id)
            .unwrap();

        assert!(metadata.is_none());
    }
}
