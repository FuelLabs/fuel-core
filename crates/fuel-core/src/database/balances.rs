use crate::database::Database;
use fuel_core_chain_config::ContractBalanceConfig;
use fuel_core_storage::{
    tables::{
        merkle::ContractsAssetsMerkleMetadata,
        ContractsAssets,
    },
    ContractsAssetKey,
    Error as StorageError,
    StorageAsRef,
    StorageBatchMutate,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::{
        AssetId,
        ContractId,
    },
};
use itertools::Itertools;

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
        let balances = balances
            .map(|(asset, balance)| {
                (ContractsAssetKey::new(contract_id, &asset), balance)
            })
            .collect_vec();
        #[allow(clippy::map_identity)]
        <_ as StorageBatchMutate<ContractsAssets>>::init_storage(
            &mut self.data,
            &mut balances.iter().map(|(key, value)| (key, value)),
        )
    }

    pub fn update_contract_balances(
        &mut self,
        balances: impl IntoIterator<Item = ContractBalanceConfig>,
    ) -> Result<(), StorageError> {
        balances
            .into_iter()
            .group_by(|s| s.contract_id)
            .into_iter()
            .try_for_each(|(contract_id, entries)| {
                if self.assets_present(&contract_id)? {
                    self.db_insert_contract_balances(entries.into_iter().collect_vec())
                } else {
                    self.init_contract_balances(
                        &contract_id,
                        entries.into_iter().map(|e| (e.asset_id, e.amount)),
                    )
                }
            })?;

        Ok(())
    }

    fn db_insert_contract_balances(
        &mut self,
        balances: impl IntoIterator<Item = ContractBalanceConfig>,
    ) -> Result<(), StorageError> {
        let balance_entries = balances
            .into_iter()
            .map(|balance_entry| {
                let db_key = ContractsAssetKey::new(
                    &balance_entry.contract_id,
                    &balance_entry.asset_id,
                );
                (db_key, balance_entry.amount)
            })
            .collect_vec();

        #[allow(clippy::map_identity)]
        let balance_entries_iter =
            balance_entries.iter().map(|(key, value)| (key, value));

        <_ as StorageBatchMutate<ContractsAssets>>::insert_batch(
            &mut self.data,
            balance_entries_iter,
        )?;

        Ok(())
    }

    fn assets_present(&mut self, key: &ContractId) -> Result<bool, StorageError> {
        Ok(self
            .storage::<ContractsAssetsMerkleMetadata>()
            .get(key)?
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        iter::repeat_with,
    };

    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::{
        tables::merkle::ContractsAssetsMerkleMetadata,
        StorageAsMut,
    };
    use fuel_core_types::fuel_types::AssetId;
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

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

        let seq_database = &mut Database::<OnChain>::default();
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

    fn random_bytes<R>(rng: &mut R) -> [u8; 32]
    where
        R: Rng + ?Sized,
    {
        rng.gen()
    }

    mod update_contract_balance {
        use fuel_core_types::{
            fuel_merkle::sparse::{
                self,
                MerkleTreeKey,
            },
            fuel_types::canonical::Deserialize,
        };

        use super::*;

        #[test]
        fn balances_inserted_into_db() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let balance_groups = repeat_with(|| ContractBalanceConfig {
                contract_id: ContractId::from_bytes(&random_bytes(&mut rng)).unwrap(),
                asset_id: AssetId::from(random_bytes(&mut rng)),
                amount: rng.gen(),
            })
            .chunks(100)
            .into_iter()
            .map(|chunk| chunk.collect_vec())
            .take(10)
            .collect_vec();

            let database = &mut Database::default();

            // when
            for group in &balance_groups {
                database
                    .update_contract_balances(group.clone())
                    .expect("Should insert contract balances");
            }

            // then
            let balances_in_db: Vec<_> = database
                .iter_all::<ContractsAssets>(None)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .map(|(key, amount)| {
                    let contract_id = *key.contract_id();
                    let asset_id = *key.asset_id();

                    ContractBalanceConfig {
                        contract_id,
                        asset_id,
                        amount,
                    }
                })
                .collect();

            let original_balances = balance_groups
                .into_iter()
                .flatten()
                .sorted()
                .collect::<Vec<_>>();

            assert_eq!(balances_in_db, original_balances);
        }

        fn merkalize(balance: &[ContractBalanceConfig]) -> [u8; 32] {
            let balance = balance.iter().map(|b| {
                let ckey = ContractsAssetKey::new(&b.contract_id, &b.asset_id);
                (MerkleTreeKey::new(ckey), b.amount.to_be_bytes())
            });
            sparse::in_memory::MerkleTree::root_from_set(balance.into_iter())
        }

        #[test]
        fn metadata_updated_single_contract() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let contract_id = ContractId::from(random_bytes(&mut rng));
            let balances = repeat_with(|| ContractBalanceConfig {
                contract_id,
                asset_id: AssetId::from(random_bytes(&mut rng)),
                amount: rng.gen(),
            })
            .take(100)
            .collect_vec();

            let database = &mut Database::default();

            // when
            database.update_contract_balances(balances.clone()).unwrap();

            // then
            let expected_root = merkalize(&balances);
            let metadata = database
                .storage::<ContractsAssetsMerkleMetadata>()
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

            let balance_per_contract = contract_ids
                .iter()
                .map(|contract_id| {
                    repeat_with(|| ContractBalanceConfig {
                        contract_id: *contract_id,
                        asset_id: AssetId::from(random_bytes(&mut rng)),
                        amount: rng.gen(),
                    })
                    .take(10)
                    .collect_vec()
                })
                .collect_vec();

            let database = &mut Database::default();

            // when
            let balances = balance_per_contract.clone().into_iter().flatten();
            database.update_contract_balances(balances).unwrap();

            // then
            let all_metadata = contract_ids
                .into_iter()
                .map(|contract_id| {
                    let root = *database
                        .storage::<ContractsAssetsMerkleMetadata>()
                        .get(&contract_id)
                        .unwrap()
                        .unwrap()
                        .root();
                    (contract_id, root)
                })
                .collect::<HashSet<_>>();

            let expected = HashSet::from([
                (contract_ids[0], merkalize(&balance_per_contract[0])),
                (contract_ids[1], merkalize(&balance_per_contract[1])),
                (contract_ids[2], merkalize(&balance_per_contract[2])),
            ]);

            assert_eq!(all_metadata, expected);
        }

        #[test]
        fn metadata_updated_multiple_contracts_shuffled() {
            // given
            let mut rng = StdRng::seed_from_u64(0);

            let contract_ids = [[1; 32], [2; 32], [3; 32]].map(ContractId::from);

            let balance_per_contract = contract_ids
                .iter()
                .map(|contract_id| {
                    repeat_with(|| ContractBalanceConfig {
                        contract_id: *contract_id,
                        asset_id: AssetId::from(random_bytes(&mut rng)),
                        amount: rng.gen(),
                    })
                    .take(10)
                    .collect_vec()
                })
                .collect_vec();

            let database = &mut Database::default();

            // when
            use itertools::Itertools;
            let contract_0_balance = balance_per_contract[0]
                .iter()
                .chunks(2)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .collect_vec();
            let contract_1_balance = balance_per_contract[1]
                .iter()
                .chunks(2)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .collect_vec();
            let contract_2_balance = balance_per_contract[2]
                .iter()
                .chunks(2)
                .into_iter()
                .map(|chunk| chunk.collect_vec())
                .collect_vec();

            let shuffled_balance = contract_0_balance
                .into_iter()
                .interleave(contract_1_balance)
                .interleave(contract_2_balance)
                .flatten()
                .cloned()
                .collect_vec();
            database.update_contract_balances(shuffled_balance).unwrap();

            // then
            let all_metadata = contract_ids
                .into_iter()
                .map(|contract_id| {
                    let root = *database
                        .storage::<ContractsAssetsMerkleMetadata>()
                        .get(&contract_id)
                        .unwrap()
                        .unwrap()
                        .root();
                    (contract_id, root)
                })
                .collect::<Vec<_>>();

            let expected = [
                (contract_ids[0], merkalize(&balance_per_contract[0])),
                (contract_ids[1], merkalize(&balance_per_contract[1])),
                (contract_ids[2], merkalize(&balance_per_contract[2])),
            ];
            assert_eq!(all_metadata, expected);
        }
    }
}
