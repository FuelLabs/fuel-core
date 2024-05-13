use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    tables::{
        merkle::ContractsAssetsMerkleMetadata,
        ContractsAssets,
    },
    ContractsAssetKey,
    Error as StorageError,
    StorageAsRef,
    StorageBatchMutate,
    StorageInspect,
};
use fuel_core_types::{
    fuel_asm::Word,
    fuel_types::{
        AssetId,
        ContractId,
    },
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
    S: StorageInspect<ContractsAssetsMerkleMetadata, Error = StorageError>,
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
                if self
                    .storage::<ContractsAssetsMerkleMetadata>()
                    .get(&contract_id)?
                    .is_some()
                {
                    let balance_entries = entries
                        .into_iter()
                        .map(|balance_entry| (balance_entry.key, balance_entry.value))
                        .collect_vec();

                    #[allow(clippy::map_identity)]
                    let balance_entries_iter =
                        balance_entries.iter().map(|(key, value)| (key, value));

                    <_ as StorageBatchMutate<ContractsAssets>>::insert_batch(
                        self,
                        balance_entries_iter,
                    )
                } else {
                    self.init_contract_balances(
                        &contract_id,
                        entries.into_iter().map(|e| (*e.key.asset_id(), e.value)),
                    )
                }
            })?;

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
    use crate::database::{
        database_description::on_chain::OnChain,
        Database,
    };
    use fuel_core_storage::{
        transactional::IntoTransaction,
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
        let mut init_database = Database::<OnChain>::default().into_transaction();

        init_database
            .init_contract_balances(&contract_id, data.clone().into_iter())
            .expect("Should init contract");
        let init_root = init_database
            .storage::<ContractsAssets>()
            .root(&contract_id)
            .expect("Should get root");

        let mut seq_database = Database::<OnChain>::default().into_transaction();
        for (asset, value) in data.iter() {
            seq_database
                .storage_as_mut::<ContractsAssets>()
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
        use fuel_core_chain_config::Randomize;
        use fuel_core_storage::{
            iter::IteratorOverTable,
            transactional::WriteTransaction,
        };
        use fuel_core_types::fuel_merkle::sparse::{
            self,
            MerkleTreeKey,
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

        fn merkalize(balance: &[TableEntry<ContractsAssets>]) -> [u8; 32] {
            let balance = balance.iter().map(|b| {
                let ckey = b.key;
                (MerkleTreeKey::new(ckey), b.value.to_be_bytes())
            });
            sparse::in_memory::MerkleTree::root_from_set(balance.into_iter())
        }

        #[test]
        fn metadata_updated_single_contract() {
            // given
            let mut rng = StdRng::seed_from_u64(0);
            let contract_id = ContractId::from(random_bytes(&mut rng));
            let balances = repeat_with(|| TableEntry {
                key: ContractsAssetKey::new(
                    &contract_id,
                    &Randomize::randomize(&mut rng),
                ),
                value: Randomize::randomize(&mut rng),
            })
            .take(100)
            .collect_vec();

            let mut database = Database::<OnChain>::default().into_transaction();

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
                    repeat_with(|| TableEntry {
                        key: ContractsAssetKey::new(
                            contract_id,
                            &Randomize::randomize(&mut rng),
                        ),
                        value: Randomize::randomize(&mut rng),
                    })
                    .take(10)
                    .collect_vec()
                })
                .collect_vec();

            let mut database = Database::<OnChain>::default().into_transaction();

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
                    repeat_with(|| TableEntry {
                        key: ContractsAssetKey::new(
                            contract_id,
                            &Randomize::randomize(&mut rng),
                        ),
                        value: Randomize::randomize(&mut rng),
                    })
                    .take(10)
                    .collect_vec()
                })
                .collect_vec();

            let mut database = Database::<OnChain>::default().into_transaction();

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
