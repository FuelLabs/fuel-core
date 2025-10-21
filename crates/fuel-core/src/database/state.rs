use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    ContractsStateKey,
    Error as StorageError,
    StorageBatchMutate,
    tables::ContractsState,
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
                self.init_contract_state(
                    &contract_id,
                    entries
                        .into_iter()
                        .map(|e| (*e.key.state_key(), e.value.into())),
                )
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{
        Database,
        database_description::on_chain::OnChain,
    };
    use fuel_core_storage::{
        StorageAsMut,
        transactional::IntoTransaction,
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
    fn init_contract_state_works() {
        use rand::{
            SeedableRng,
            rngs::StdRng,
        };

        let rng = &mut StdRng::seed_from_u64(1234);
        let r#gen = || Some((random_bytes32(rng), random_bytes32(rng).to_vec()));
        let data = core::iter::from_fn(r#gen).take(5_000).collect::<Vec<_>>();

        let contract_id = ContractId::from([1u8; 32]);
        let mut init_database = Database::<OnChain>::default().into_transaction();

        init_database
            .init_contract_state(&contract_id, data.clone().into_iter())
            .expect("Should init contract");

        let mut seq_database = Database::<OnChain>::default().into_transaction();
        for (key, value) in data.iter() {
            seq_database
                .storage_as_mut::<ContractsState>()
                .insert(&ContractsStateKey::new(&contract_id, key), value)
                .expect("Should insert a state");
        }

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
            let value = value.into();
            assert_eq!(init_value, value);
            assert_eq!(seq_value, value);
        }
    }

    mod update_contract_state {
        use core::iter::repeat_with;
        use fuel_core_chain_config::{
            ContractStateConfig,
            Randomize,
        };

        use fuel_core_storage::iter::IteratorOverTable;
        use rand::{
            SeedableRng,
            rngs::StdRng,
        };

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
                    value: value.into(),
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
    }
}
