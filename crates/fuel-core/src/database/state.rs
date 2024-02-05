use crate::database::Database;
use fuel_core_storage::{
    tables::ContractsState,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::StorageAsMut;
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
}
