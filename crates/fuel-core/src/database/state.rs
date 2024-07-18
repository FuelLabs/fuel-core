use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        merkle::ContractsStateMerkleMetadata,
        ContractsState,
    },
    ContractsStateKey,
    Error as StorageError,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
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
    S: StorageMutate<ContractsState, Error = StorageError>,
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
        for (key, value) in slots.iter() {
            self.insert(key, value)?;
        }
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

                    for (key, value) in state_entries.iter() {
                        self.insert(key, value)?;
                    }
                    Ok(())
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
