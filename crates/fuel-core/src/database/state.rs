use super::utils::MerkleTreeDbUtils;
use crate::database::Database;
use fuel_core_chain_config::ContractStateConfig;
use fuel_core_storage::{
    tables::{
        merkle::{
            ContractsStateMerkleData,
            ContractsStateMerkleMetadata,
        },
        ContractsState,
    },
    ContractsStateKey,
    Error as StorageError,
    StorageBatchMutate,
};
use fuel_core_types::fuel_types::{
    Bytes32,
    ContractId,
};
use itertools::Itertools;

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
        let slots = slots
            .map(|(key, value)| (ContractsStateKey::new(contract_id, &key), value))
            .collect_vec();
        #[allow(clippy::map_identity)]
        <_ as StorageBatchMutate<ContractsState>>::init_storage(
            &mut self.data,
            &mut slots.iter().map(|(key, value)| (key, value)),
        )
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
                >::update(self.as_mut(), &contract_id, slots)
            })?;
        Ok(())
    }

    fn db_insert_contract_states(
        &mut self,
        slots: &[ContractStateConfig],
    ) -> Result<(), StorageError> {
        let state_entries = slots
            .iter()
            .map(|state_entry| {
                let contract_id = ContractId::from(*state_entry.contract_id);

                let db_key = ContractsStateKey::new(&contract_id, &state_entry.key);
                (db_key, state_entry.value)
            })
            .collect_vec();

        let state_entries_iter = state_entries.iter().map(|(key, value)| (key, value));

        // TODO dont collect
        <Database as StorageBatchMutate<ContractsState>>::insert_batch(
            self.as_mut(),
            state_entries_iter,
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        iter::repeat_with,
    };

    use crate::database::database_description::on_chain::OnChain;

    use super::*;
    use fuel_core_storage::{
        StorageAsMut,
        StorageMutate,
    };
    use fuel_core_types::fuel_merkle::sparse;
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

    fn random_contract_id(rng: &mut impl Rng) -> ContractId {
        ContractId::new(rng.gen())
    }

    #[test]
    fn get() {
        let key = (&ContractId::from([1u8; 32]), &Bytes32::from([1u8; 32])).into();
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = &mut Database::<OnChain>::default();
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

        let database = &mut Database::<OnChain>::default();
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

        let database = &mut Database::<OnChain>::default();
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

        let database = &mut Database::<OnChain>::default();
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

        let mut database = Database::<OnChain>::default();

        StorageMutate::<ContractsState>::insert(&mut database, &key, &stored_value)
            .unwrap();

        let root = database.storage::<ContractsState>().root(key.contract_id());
        assert!(root.is_ok())
    }

    #[test]
    fn root_returns_empty_root_for_invalid_contract() {
        let invalid_contract_id = ContractId::from([1u8; 32]);
        let mut database = Database::<OnChain>::default();
        let empty_root = sparse::in_memory::MerkleTree::new().root();
        let root = database
            .storage::<ContractsState>()
            .root(&invalid_contract_id)
            .unwrap();
        assert_eq!(root, empty_root)
    }

    #[test]
    fn put_updates_the_state_merkle_root_for_the_given_contract() {
        let contract_id = ContractId::from([1u8; 32]);
        let database = &mut Database::<OnChain>::default();

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
        let database = &mut Database::<OnChain>::default();

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
        let database = &mut Database::<OnChain>::default();

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
        let database = &mut Database::<OnChain>::default();

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
        let init_database = &mut Database::<OnChain>::default();

        init_database
            .init_contract_state(&contract_id, data.clone().into_iter())
            .expect("Should init contract");
        let init_root = init_database
            .storage::<ContractsState>()
            .root(&contract_id)
            .expect("Should get root");

        let seq_database = &mut Database::<OnChain>::default();
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
        use fuel_core_types::fuel_merkle::sparse::{
            self,
            MerkleTreeKey,
        };

        use super::*;
        #[test]
        fn states_inserted_into_db() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let state_groups = repeat_with(|| ContractStateConfig {
                contract_id: random_contract_id(&mut rng),
                key: random_bytes32(&mut rng),
                value: random_bytes32(&mut rng),
            })
            .chunks(100)
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .take(10)
            .collect_vec();

            let database = &mut Database::<OnChain>::default();

            // when
            for group in &state_groups {
                database
                    .update_contract_states(group.clone())
                    .expect("Should insert contract state");
            }

            // then
            let states_in_db: Vec<_> = database
                .iter_all::<ContractsState>(None)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .map(|(key, value)| {
                    let contract_id = *key.contract_id();
                    let key = *key.state_key();

                    ContractStateConfig {
                        contract_id,
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
            let contract_id = random_contract_id(&mut rng);
            let state = repeat_with(|| ContractStateConfig {
                contract_id,
                key: random_bytes32(&mut rng),
                value: random_bytes32(&mut rng),
            })
            .take(100)
            .collect_vec();

            let database = &mut Database::<OnChain>::default();

            // when
            database.update_contract_states(state.clone()).unwrap();

            // then
            let expected_root = merkalize(&state);
            let metadata = database
                .storage::<ContractsStateMerkleMetadata>()
                .get(&contract_id)
                .unwrap()
                .unwrap();

            assert_eq!(*metadata.root(), expected_root);
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
                        contract_id: *id,
                        key: random_bytes32(&mut rng),
                        value: random_bytes32(&mut rng),
                    })
                    .take(10)
                    .collect_vec()
                })
                .collect_vec();

            let database = &mut Database::<OnChain>::default();

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
                        .root();
                    (contract_id, *root)
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
                contract_id,
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

            let database = &mut Database::<OnChain>::default();

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
                        .root();
                    (contract_id, *root)
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
        let database = &mut Database::<OnChain>::default();

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
