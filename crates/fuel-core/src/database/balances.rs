use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    ContractsAssetKey, Error as StorageError, StorageBatchMutate, tables::ContractsAssets,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::{AssetId, ContractId},
};
use itertools::Itertools;

pub trait BalancesInitializer {
    /// Initialize the balances of the contract from the all leaves.
    /// This method is more performant than inserting balances one by one.
    fn init_contract_balances<S>(
        &mut self,
        contract_id: &ContractId,
        balances: S,
    ) -> Result<(), StorageError>
    where
        S: Iterator<Item = (AssetId, Word)>;

    fn update_contract_balances(
        &mut self,
        balances: impl IntoIterator<Item = TableEntry<ContractsAssets>>,
    ) -> Result<(), StorageError>;
}

impl<S> BalancesInitializer for S
where
    S: StorageBatchMutate<ContractsAssets, Error = StorageError>,
{
    fn init_contract_balances<I>(
        &mut self,
        contract_id: &ContractId,
        balances: I,
    ) -> Result<(), StorageError>
    where
        I: Iterator<Item = (AssetId, Word)>,
    {
        let balances = balances
            .map(|(asset, balance)| {
                (ContractsAssetKey::new(contract_id, &asset), balance)
            })
            .collect_vec();
        #[allow(clippy::map_identity)]
        <_ as StorageBatchMutate<ContractsAssets>>::init_storage(
            self,
            &mut balances.iter().map(|(key, value)| (key, value)),
        )
    }

    fn update_contract_balances(
        &mut self,
        balances: impl IntoIterator<Item = TableEntry<ContractsAssets>>,
    ) -> Result<(), StorageError> {
        balances
            .into_iter()
            .group_by(|s| *s.key.contract_id())
            .into_iter()
            .try_for_each(|(contract_id, entries)| {
                self.init_contract_balances(
                    &contract_id,
                    entries.into_iter().map(|e| (*e.key.asset_id(), e.value)),
                )
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::iter::repeat_with;

    use super::*;
    use crate::database::{Database, database_description::on_chain::OnChain};
    use fuel_core_storage::{StorageAsMut, transactional::IntoTransaction};
    use fuel_core_types::fuel_types::AssetId;
    use rand::{Rng, SeedableRng, rngs::StdRng};

    fn random_asset_id<R>(rng: &mut R) -> AssetId
    where
        R: Rng + ?Sized,
    {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        bytes.into()
    }

    #[test]
    fn init_contract_balances_works() {
        use rand::{RngCore, SeedableRng, rngs::StdRng};

        let rng = &mut StdRng::seed_from_u64(1234);
        let r#gen = || Some((random_asset_id(rng), rng.next_u64()));
        let data = core::iter::from_fn(r#gen).take(5_000).collect::<Vec<_>>();

        let contract_id = ContractId::from([1u8; 32]);
        let mut init_database = Database::<OnChain>::default().into_transaction();

        init_database
            .init_contract_balances(&contract_id, data.clone().into_iter())
            .expect("Should init contract");

        let mut seq_database = Database::<OnChain>::default().into_transaction();
        for (asset, value) in data.iter() {
            seq_database
                .storage_as_mut::<ContractsAssets>()
                .insert(&ContractsAssetKey::new(&contract_id, asset), value)
                .expect("Should insert a state");
        }

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

    mod update_contract_balance {
        use fuel_core_chain_config::Randomize;
        use fuel_core_storage::{
            iter::IteratorOverTable, transactional::WriteTransaction,
        };

        use super::*;

        #[test]
        fn balances_inserted_into_db() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let balance_groups = repeat_with(|| TableEntry::randomize(&mut rng))
                .chunks(100)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .take(10)
                .collect_vec();

            let mut database = Database::<OnChain>::default();
            let mut transaction = database.write_transaction();

            // when
            for group in &balance_groups {
                transaction
                    .update_contract_balances(group.clone())
                    .expect("Should insert contract balances");
            }
            transaction.commit().unwrap();

            // then
            let balances_in_db: Vec<_> = database
                .iter_all::<ContractsAssets>(None)
                .map_ok(|(k, v)| TableEntry { key: k, value: v })
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .collect();

            let original_balances = balance_groups
                .into_iter()
                .flatten()
                .sorted_by_key(|e| e.key)
                .collect::<Vec<_>>();

            assert_eq!(balances_in_db, original_balances);
        }
    }
}
