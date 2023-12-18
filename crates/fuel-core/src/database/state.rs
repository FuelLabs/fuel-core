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
use fuel_core_chain_config::ContractStateConfig;
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

use super::utils::MerkleTreeDbUtils;

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
        let prev = Database::remove(self, key.as_ref(), Column::ContractsState)
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
    // Insert a batch of contract state slots into the database and update the merkle root
    pub fn batch_insert_contract_state(
        &mut self,
        contract_id: &ContractId,
        slots: Vec<(Bytes32, Bytes32)>,
    ) -> Result<(), StorageError> {
        if slots.is_empty() {
            return Ok(());
        }

        let state_entries = slots
            .iter()
            .map(|(key, value)| (ContractsStateKey::new(contract_id, key), value));
        self.batch_insert(Column::ContractsState, state_entries)
            .unwrap();

        let slots = slots
            .into_iter()
            .map(|(key, value)| (MerkleTreeKey::new(key), value));

        // fetch the current merkle tree and update it with the batch of slots
        let root = self
            .storage::<ContractsStateMerkleMetadata>()
            .get(contract_id)?
            .map(|metadata| metadata.root)
            .unwrap_or_else(|| in_memory::MerkleTree::new().root());
        let storage = self.borrow_mut();
        let mut tree: MerkleTree<ContractsStateMerkleData, _> = MerkleTree::load(
            storage, &root,
        )
        .map_err(|err: sparse::MerkleTreeError<StorageError>| {
            StorageError::Other(anyhow::anyhow!("{err:?}"))
        })?;

        for (key, value) in slots {
            tree.update(key, value.as_slice())
                .map_err(|err| StorageError::Other(anyhow::anyhow!("{err:?}")))?;
        }

        // Generate new metadata for the updated tree
        let root = tree.root();
        let metadata = SparseMerkleMetadata { root };
        self.storage::<ContractsStateMerkleMetadata>()
            .insert(contract_id, &metadata)?;

        Ok(())
    }

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
            return Ok(());
        }

        if self
            .storage::<ContractsStateMerkleMetadata>()
            .contains_key(contract_id)?
        {
            return Err(
                anyhow::anyhow!("The contract state is already initialized").into()
            );
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

    /// Updates the state of multiple contracts based on provided state slots.
    ///
    /// Grouping: Adjacent state entries sharing the same contract ID are grouped together.
    ///           This ensures that consecutive entries for the same contract are processed as a single batch.
    ///
    /// State Update Process:
    ///    - All state entries are inserted into the database.
    ///    - For new contracts (i.e., those without a previously recorded state), the group is
    ///      first sorted before the state root is calculated. This is a consequence of the
    ///      batch-insertion logic of MerkleTree::from_set.
    ///    - For contracts with an existing state, the function updates their state merkle tree
    ///      calling MerkleTree::update for each state entry in the group in-order.
    ///
    /// # Errors
    /// On any error while accessing the database.
    pub fn update_contract_states(
        &mut self,
        slots: impl IntoIterator<Item = ContractStateConfig>,
    ) -> Result<(), StorageError> {
        let slots = slots.into_iter().collect_vec();

        self.db_insert_contract_states(&slots)?;

        self.update_state_merkle_tree(slots)?;

        Ok(())
    }

    fn update_state_merkle_tree(
        &mut self,
        slots: Vec<ContractStateConfig>,
    ) -> Result<(), StorageError> {
        slots
            .into_iter()
            .group_by(|s| s.contract_id)
            .into_iter()
            .map(|(contract_id, slots)| (contract_id, slots.map(|s| (s.key, s.value))))
            .try_for_each(|(contract_id, slots)| {
                let contract_id = ContractId::from(*contract_id);
                MerkleTreeDbUtils::<
                    ContractsStateMerkleMetadata,
                    ContractsStateMerkleData,
                >::update(self.borrow_mut(), &contract_id, slots)
            })?;
        Ok(())
    }

    fn db_insert_contract_states(
        &mut self,
        slots: &[ContractStateConfig],
    ) -> Result<(), StorageError> {
        let state_entries = slots.iter().map(|state_entry| {
            let contract_id = ContractId::from(*state_entry.contract_id);

            let db_key = ContractsStateKey::new(&contract_id, &state_entry.key);
            (db_key, state_entry.value)
        });

        self.batch_insert(Column::ContractsState, state_entries)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        iter::repeat_with,
    };

    use super::*;
    use fuel_core_storage::{
        StorageAsMut,
        StorageAsRef,
    };
    use fuel_core_types::fuel_types::{
        canonical::Deserialize,
        Bytes32,
    };
    use rand::{
        self,
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

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

    mod update_contract_state {
        use super::*;
        #[test]
        fn states_inserted_into_db() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let state_groups = repeat_with(|| ContractStateConfig {
                contract_id: random_bytes32(&mut rng),
                key: random_bytes32(&mut rng),
                value: random_bytes32(&mut rng),
            })
            .chunks(100)
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .take(10)
            .collect_vec();

            let database = &mut Database::default();

            // when
            for group in &state_groups {
                database
                    .update_contract_states(group.clone())
                    .expect("Should insert contract state");
            }

            // then
            let states_in_db: Vec<_> = database
                .iter_all(Column::ContractsState, None)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .map(|(key, value): (Vec<u8>, Vec<u8>)| {
                    let contract_id = &key[..32];
                    let state_id = &key[32..];

                    let key = Bytes32::from_bytes(state_id).unwrap();
                    let value = Bytes32::from_bytes(&value).unwrap();
                    ContractStateConfig {
                        contract_id: Bytes32::from_bytes(contract_id).unwrap(),
                        key,
                        value,
                    }
                })
                .collect();

            let original_state = state_groups
                .into_iter()
                .flatten()
                .sorted()
                .collect::<Vec<_>>();

            assert_eq!(states_in_db, original_state);
        }

        fn merkalize(state: &[ContractStateConfig]) -> [u8; 32] {
            let state = state.iter().map(|s| (MerkleTreeKey::new(s.key), s.value));
            sparse::in_memory::MerkleTree::nodes_from_set(state.into_iter()).0
        }

        #[test]
        fn metadata_updated_single_contract() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let contract_id = ContractId::from(*random_bytes32(&mut rng));
            let state = repeat_with(|| ContractStateConfig {
                contract_id: Bytes32::from(*contract_id),
                key: random_bytes32(&mut rng),
                value: random_bytes32(&mut rng),
            })
            .take(100)
            .collect_vec();

            let database = &mut Database::default();

            // when
            database.update_contract_states(state.clone()).unwrap();

            // then
            let expected_root = merkalize(&state);
            let metadata = database
                .storage::<ContractsStateMerkleMetadata>()
                .get(&contract_id)
                .unwrap()
                .unwrap();

            assert_eq!(metadata.root, expected_root);
        }

        #[test]
        fn metadata_updated_multiple_contracts() {
            // given
            let mut rng = StdRng::seed_from_u64(0);

            let contract_ids =
                [[1; 32], [2; 32], [3; 32]].map(|bytes| ContractId::from(bytes));

            let state_per_contract = contract_ids
                .iter()
                .map(|id| {
                    repeat_with(|| ContractStateConfig {
                        contract_id: Bytes32::from(**id),
                        key: random_bytes32(&mut rng),
                        value: random_bytes32(&mut rng),
                    })
                    .take(10)
                    .collect_vec()
                })
                .collect_vec();

            let database = &mut Database::default();

            // when
            let states = state_per_contract.clone().into_iter().flatten();
            database.update_contract_states(states).unwrap();

            // then
            let all_metadata = contract_ids
                .into_iter()
                .map(|contract_id| {
                    let root = database
                        .storage::<ContractsStateMerkleMetadata>()
                        .get(&contract_id)
                        .unwrap()
                        .unwrap()
                        .root;
                    (contract_id, root)
                })
                .collect::<HashSet<_>>();

            let expected = HashSet::from([
                (contract_ids[0], merkalize(&state_per_contract[0])),
                (contract_ids[1], merkalize(&state_per_contract[1])),
                (contract_ids[2], merkalize(&state_per_contract[2])),
            ]);

            assert_eq!(all_metadata, expected);
        }

        #[test]
        fn metadata_updated_multiple_contracts_shuffled() {
            // given
            let mut rng = StdRng::seed_from_u64(0);

            let contract_ids =
                [[1; 32], [2; 32], [3; 32]].map(|bytes| ContractId::from(bytes));
            let mut random_state = |contract_id: ContractId| ContractStateConfig {
                contract_id: Bytes32::from(*contract_id),
                key: random_bytes32(&mut rng),
                value: random_bytes32(&mut rng),
            };
            let state_per_contract = contract_ids
                .iter()
                .map(|id| {
                    repeat_with(|| random_state(*id))
                        .take(10)
                        .sorted()
                        .collect_vec()
                })
                .collect_vec();

            let database = &mut Database::default();

            // when
            use itertools::Itertools;
            let contract_0_state = state_per_contract[0]
                .iter()
                .chunks(2)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .collect_vec();
            let contract_1_state = state_per_contract[1]
                .iter()
                .chunks(2)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .collect_vec();
            let contract_2_state = state_per_contract[2]
                .iter()
                .chunks(2)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .collect_vec();

            let shuffled_state = contract_0_state
                .into_iter()
                .interleave(contract_1_state.into_iter())
                .interleave(contract_2_state.into_iter())
                .flatten()
                .cloned()
                .into_iter()
                .collect_vec();

            database.update_contract_states(shuffled_state).unwrap();

            // then
            let all_metadata = contract_ids
                .into_iter()
                .map(|contract_id| {
                    let root = database
                        .storage::<ContractsStateMerkleMetadata>()
                        .get(&contract_id)
                        .unwrap()
                        .unwrap()
                        .root;
                    (contract_id, root)
                })
                .collect::<Vec<_>>();

            let expected = [
                (contract_ids[0], merkalize(&state_per_contract[0])),
                (contract_ids[1], merkalize(&state_per_contract[1])),
                (contract_ids[2], merkalize(&state_per_contract[2])),
            ];
            assert_eq!(all_metadata, expected);
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
