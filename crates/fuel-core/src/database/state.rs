use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        merkle::ContractsStateMerkleMetadata,
        ContractsState,
    },
    ContractsStateKey,
    Error as StorageError,
    StorageAsRef,
    StorageBatchMutate,
    StorageInspect,
};
use fuel_core_types::fuel_types::{
    Bytes32,
    ContractId,
};
use itertools::Itertools;

pub trait StateInitializer {
    /// Initialize the state of the contract from all leaves.
    /// This method is more performant than inserting state one by one.
    fn init_contract_state<S>(
        &mut self,
        contract_id: &ContractId,
        slots: S,
    ) -> Result<(), StorageError>
    where
        S: Iterator<Item = (Bytes32, Vec<u8>)>;

    /// Updates the state of multiple contracts based on provided state slots.
    fn update_contract_states(
        &mut self,
        states: impl IntoIterator<Item = TableEntry<ContractsState>>,
    ) -> Result<(), StorageError>;
}

impl<S> StateInitializer for S
where
    S: StorageInspect<ContractsStateMerkleMetadata, Error = StorageError>,
    S: StorageBatchMutate<ContractsState, Error = StorageError>,
{
    fn init_contract_state<I>(
        &mut self,
        contract_id: &ContractId,
        slots: I,
    ) -> Result<(), StorageError>
    where
        I: Iterator<Item = (Bytes32, Vec<u8>)>,
    {
        let slots = slots
            .map(|(key, value)| (ContractsStateKey::new(contract_id, &key), value))
            .collect_vec();
        <_ as StorageBatchMutate<ContractsState>>::init_storage(
            self,
            &mut slots.iter().map(|(key, value)| (key, value.as_slice())),
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
    fn update_contract_states(
        &mut self,
        states: impl IntoIterator<Item = TableEntry<ContractsState>>,
    ) -> Result<(), StorageError> {
        states
            .into_iter()
            .group_by(|s| *s.key.contract_id())
            .into_iter()
            .try_for_each(|(contract_id, entries)| {
                if self
                    .storage::<ContractsStateMerkleMetadata>()
                    .get(&contract_id)?
                    .is_some()
                {
                    // TODO: this collecting is unfortunate. We should try to avoid it.
                    let state_entries = entries
                        .into_iter()
                        .map(|state_entry| {
                            (state_entry.key, Vec::<u8>::from(state_entry.value))
                        })
                        .collect_vec();

                    <_ as StorageBatchMutate<ContractsState>>::insert_batch(
                        self,
                        state_entries
                            .iter()
                            .map(|entry| (&entry.0, entry.1.as_slice())),
                    )
                } else {
                    self.init_contract_state(
                        &contract_id,
                        entries
                            .into_iter()
                            .map(|e| (*e.key.state_key(), e.value.into())),
                    )
                }
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{
        database_description::on_chain::OnChain,
        Database,
    };
    use fuel_core_storage::{
        transactional::IntoTransaction,
        StorageAsMut,
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

    fn random_contract_id(rng: &mut impl Rng) -> ContractId {
        ContractId::new(rng.gen())
    }

    #[test]
    fn init_contract_state_works() {
        use rand::{
            rngs::StdRng,
            SeedableRng,
        };

        let rng = &mut StdRng::seed_from_u64(1234);
        let gen = || Some((random_bytes32(rng), random_bytes32(rng).to_vec()));
        let data = core::iter::from_fn(gen).take(5_000).collect::<Vec<_>>();

        let contract_id = ContractId::from([1u8; 32]);
        let mut init_database = Database::<OnChain>::default().into_transaction();

        init_database
            .init_contract_state(&contract_id, data.clone().into_iter())
            .expect("Should init contract");
        let init_root = init_database
            .storage::<ContractsState>()
            .root(&contract_id)
            .expect("Should get root");

        let mut seq_database = Database::<OnChain>::default().into_transaction();
        for (key, value) in data.iter() {
            seq_database
                .storage_as_mut::<ContractsState>()
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
            assert_eq!(init_value.0, value);
            assert_eq!(seq_value.0, value);
        }
    }

    mod update_contract_state {
        use core::iter::repeat_with;
        use fuel_core_chain_config::{
            ContractStateConfig,
            Randomize,
        };

        use fuel_core_storage::iter::IteratorOverTable;
        use fuel_core_types::fuel_merkle::sparse::{
            self,
            MerkleTreeKey,
        };
        use rand::{
            rngs::StdRng,
            SeedableRng,
        };
        use std::collections::HashSet;

        use super::*;

        #[test]
        fn states_inserted_into_db() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let state_groups = repeat_with(|| TableEntry::randomize(&mut rng))
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
            let mut states_in_db: Vec<_> = database
                .iter_all::<ContractsState>(None)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .map(|(key, value)| ContractStateConfig {
                    key: *key.state_key(),
                    value: value.0,
                })
                .collect();

            let mut original_state = state_groups
                .into_iter()
                .flatten()
                .map(|entry| ContractStateConfig {
                    key: *entry.key.state_key(),
                    value: entry.value.into(),
                })
                .collect::<Vec<_>>();

            states_in_db.sort();
            original_state.sort();

            assert_eq!(states_in_db, original_state);
        }

        fn merkalize(state: &[TableEntry<ContractsState>]) -> [u8; 32] {
            let state = state.iter().map(|s| {
                let ckey = s.key;
                (MerkleTreeKey::new(ckey), &s.value)
            });
            sparse::in_memory::MerkleTree::root_from_set(state.into_iter())
        }

        #[test]
        fn metadata_updated_single_contract() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let contract_id = random_contract_id(&mut rng);
            let state = repeat_with(|| TableEntry {
                key: ContractsStateKey::new(
                    &contract_id,
                    &Randomize::randomize(&mut rng),
                ),
                value: Randomize::randomize(&mut rng),
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

            let contract_ids = [[1; 32], [2; 32], [3; 32]].map(ContractId::from);

            let state_per_contract = contract_ids
                .iter()
                .map(|id| {
                    repeat_with(|| TableEntry {
                        key: ContractsStateKey::new(id, &Randomize::randomize(&mut rng)),
                        value: Randomize::randomize(&mut rng),
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
                    let root = *database
                        .storage::<ContractsStateMerkleMetadata>()
                        .get(&contract_id)
                        .unwrap()
                        .unwrap()
                        .root();
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

            let contract_ids = [[1; 32], [2; 32], [3; 32]].map(ContractId::from);
            let mut random_state = |contract_id: ContractId| TableEntry {
                key: ContractsStateKey::new(
                    &contract_id,
                    &Randomize::randomize(&mut rng),
                ),
                value: Randomize::randomize(&mut rng),
            };
            let state_per_contract = contract_ids
                .iter()
                .map(|id| {
                    repeat_with(|| random_state(*id))
                        .take(10)
                        .sorted_by_key(|e| e.key)
                        .collect_vec()
                })
                .collect_vec();

            let database = &mut Database::<OnChain>::default();

            // when
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
                .interleave(contract_1_state)
                .interleave(contract_2_state)
                .flatten()
                .cloned()
                .collect_vec();

            database.update_contract_states(shuffled_state).unwrap();

            // then
            let all_metadata = contract_ids
                .into_iter()
                .map(|contract_id| {
                    let root = *database
                        .storage::<ContractsStateMerkleMetadata>()
                        .get(&contract_id)
                        .unwrap()
                        .unwrap()
                        .root();
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
}
